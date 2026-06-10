//! HEVC encoder via a Media Foundation hardware MFT (spec §12).
//!
//! On the RTX 4070 `MFTEnumEx` resolves the NVIDIA NVENC HEVC encoder, which is
//! an **async** MFT driven by `METransformNeedInput` / `METransformHaveOutput`
//! events. Input is an NV12 D3D11 texture (shared device via `IMFDXGIDeviceManager`);
//! output is Annex B HEVC with PTS=DTS (B-frames disabled).

use super::{VideoEncoder, VideoEncoderConfig};
use crate::capture::GridFrame;
use crate::mux::{EncodedPacket, StreamKind};
use anyhow::{anyhow, Context, Result};
use rec_core::timebase::ns100_to_pts90k;
use std::mem::ManuallyDrop;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::core::VARIANT;
use windows::Win32::Media::MediaFoundation::*;

const ME_TRANSFORM_NEED_INPUT: u32 = 601;
const ME_TRANSFORM_HAVE_OUTPUT: u32 = 602;
const ME_TRANSFORM_DRAIN_COMPLETE: u32 = 603;

pub struct MfHevcEncoder {
    transform: IMFTransform,
    events: IMFMediaEventGenerator,
    _manager: IMFDXGIDeviceManager,
    codec_private: Vec<u8>,
    duration_100ns: i64,
}

// SAFETY: the encoder is owned and driven by a single dedicated encode thread.
// The MF interfaces are only ever touched from that one thread after the move.
unsafe impl Send for MfHevcEncoder {}

impl MfHevcEncoder {
    pub fn new(cfg: VideoEncoderConfig, device: &ID3D11Device) -> Result<Self> {
        crate::win::init_mf();

        let transform = enumerate_hardware_encoder()?;

        // Unlock async hardware MFT.
        unsafe {
            let attrs = transform.GetAttributes()?;
            attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
        }

        // Share our D3D11 device with the encoder.
        let manager = create_device_manager(device)?;
        unsafe {
            transform.ProcessMessage(
                MFT_MESSAGE_SET_D3D_MANAGER,
                manager.as_raw() as usize,
            )?;
        }

        configure_output(&transform, &cfg)?;
        configure_input(&transform, &cfg)?;
        try_configure_codec_api(&transform, &cfg);

        let events: IMFMediaEventGenerator = transform.cast()?;

        unsafe {
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
        }

        let codec_private = read_sequence_header(&transform).unwrap_or_default();
        let duration_100ns =
            (cfg.fps_den as i64 * 10_000_000) / cfg.fps_num.max(1) as i64;

        Ok(MfHevcEncoder {
            transform,
            events,
            _manager: manager,
            codec_private,
            duration_100ns,
        })
    }

    fn deliver(&mut self, frame: &GridFrame) -> Result<()> {
        let pts_100ns = frame.pts_100ns;
        unsafe {
            let buffer = MFCreateDXGISurfaceBuffer(
                &ID3D11Texture2D::IID,
                &frame.texture,
                0,
                false,
            )?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(pts_100ns)?;
            sample.SetSampleDuration(self.duration_100ns)?;
            self.transform
                .ProcessInput(0, &sample, 0)
                .context("ProcessInput")?;
        }
        Ok(())
    }

    fn drain_output(&mut self, out: &mut Vec<EncodedPacket>) -> Result<()> {
        unsafe {
            let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(None),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            }];
            let mut status = 0u32;
            self.transform
                .ProcessOutput(0, &mut buffers, &mut status)
                .context("ProcessOutput")?;

            let sample = ManuallyDrop::into_inner(std::mem::replace(
                &mut buffers[0].pSample,
                ManuallyDrop::new(None),
            ));
            let Some(sample) = sample else {
                return Ok(());
            };

            let keyframe = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
            let time_100ns = sample.GetSampleTime().unwrap_or(0);
            let pts_90k = ns100_to_pts90k(time_100ns);

            let media_buffer = sample.ConvertToContiguousBuffer()?;
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            media_buffer.Lock(&mut ptr, None, Some(&mut len))?;
            let data = std::slice::from_raw_parts(ptr, len as usize).to_vec();
            media_buffer.Unlock()?;

            out.push(EncodedPacket {
                data,
                pts_90k,
                dts_90k: pts_90k, // B-frames disabled => DTS == PTS
                keyframe,
                kind: StreamKind::Video,
            });
        }
        Ok(())
    }
}

impl VideoEncoder for MfHevcEncoder {
    fn encode(&mut self, frame: GridFrame, out: &mut Vec<EncodedPacket>) -> Result<()> {
        // Block on MFT events until it asks for input, draining outputs meanwhile.
        loop {
            let event = unsafe { self.events.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))? };
            match unsafe { event.GetType()? } {
                ME_TRANSFORM_NEED_INPUT => {
                    self.deliver(&frame)?;
                    return Ok(());
                }
                ME_TRANSFORM_HAVE_OUTPUT => self.drain_output(out)?,
                _ => {}
            }
        }
    }

    fn flush(&mut self, out: &mut Vec<EncodedPacket>) -> Result<()> {
        unsafe {
            self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)?;
        }
        // Drain until the MFT reports it has finished.
        loop {
            let event = match unsafe {
                self.events.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
            } {
                Ok(e) => e,
                Err(_) => break,
            };
            match unsafe { event.GetType()? } {
                ME_TRANSFORM_HAVE_OUTPUT => self.drain_output(out)?,
                ME_TRANSFORM_DRAIN_COMPLETE => break,
                _ => {}
            }
        }
        unsafe {
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
        }
        Ok(())
    }

    fn codec_private(&self) -> Vec<u8> {
        // Re-read live: the sequence header is populated once streaming has begun.
        read_sequence_header(&self.transform).unwrap_or_else(|| self.codec_private.clone())
    }
}

// ---------------------------------------------------------------------------

fn enumerate_hardware_encoder() -> Result<IMFTransform> {
    let output_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_HEVC,
    };
    let input_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };

    unsafe {
        let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_ASYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER,
            Some(&input_info),
            Some(&output_info),
            &mut activates,
            &mut count,
        )?;

        if count == 0 || activates.is_null() {
            return Err(anyhow!("no hardware HEVC encoder MFT found"));
        }

        let slice = std::slice::from_raw_parts(activates, count as usize);
        let activate = slice[0]
            .clone()
            .ok_or_else(|| anyhow!("null IMFActivate"))?;
        let transform: IMFTransform = activate.ActivateObject()?;

        // Free the array allocated by MFTEnumEx.
        windows::Win32::System::Com::CoTaskMemFree(Some(activates as *const _));
        Ok(transform)
    }
}

fn create_device_manager(device: &ID3D11Device) -> Result<IMFDXGIDeviceManager> {
    unsafe {
        let mut token = 0u32;
        let mut manager: Option<IMFDXGIDeviceManager> = None;
        MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
        let manager = manager.ok_or_else(|| anyhow!("null device manager"))?;
        manager.ResetDevice(device, token)?;
        Ok(manager)
    }
}

fn configure_output(transform: &IMFTransform, cfg: &VideoEncoderConfig) -> Result<()> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_HEVC)?;
        t.SetUINT32(&MF_MT_AVG_BITRATE, cfg.bitrate_bps)?;
        t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        set_size(&t, &MF_MT_FRAME_SIZE, cfg.width, cfg.height)?;
        set_ratio(&t, &MF_MT_FRAME_RATE, cfg.fps_num, cfg.fps_den)?;
        transform.SetOutputType(0, &t, 0).context("SetOutputType(HEVC)")?;
    }
    Ok(())
}

fn configure_input(transform: &IMFTransform, cfg: &VideoEncoderConfig) -> Result<()> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        set_size(&t, &MF_MT_FRAME_SIZE, cfg.width, cfg.height)?;
        set_ratio(&t, &MF_MT_FRAME_RATE, cfg.fps_num, cfg.fps_den)?;
        transform.SetInputType(0, &t, 0).context("SetInputType(NV12)")?;
    }
    Ok(())
}

/// Best-effort CBR + GOP + low-latency + no B-frames via ICodecAPI.
fn try_configure_codec_api(transform: &IMFTransform, cfg: &VideoEncoderConfig) {
    let Ok(codec) = transform.cast::<ICodecAPI>() else {
        return;
    };
    unsafe {
        // Rate control: CBR == 0, PeakConstrainedVBR == 2.
        let rc_mode: i32 = if cfg.cbr { 0 } else { 2 };
        let _ = codec.SetValue(&CODECAPI_AVEncCommonRateControlMode, &VARIANT::from(rc_mode));
        let _ = codec.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &VARIANT::from(cfg.bitrate_bps as i32));
        if !cfg.cbr {
            // Allow peaks up to 1.5x the mean for VBR.
            let _ = codec.SetValue(
                &CODECAPI_AVEncCommonMaxBitRate,
                &VARIANT::from((cfg.bitrate_bps as i32).saturating_mul(3) / 2),
            );
        }
        let _ = codec.SetValue(&CODECAPI_AVEncMPVGOPSize, &VARIANT::from(cfg.gop as i32));
        let _ = codec.SetValue(&CODECAPI_AVEncCommonLowLatency, &VARIANT::from(true));
    }
}

fn read_sequence_header(transform: &IMFTransform) -> Option<Vec<u8>> {
    unsafe {
        let t = transform.GetOutputCurrentType(0).ok()?;
        let mut len = 0u32;
        // Probe length, then read the blob.
        if t.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER).map(|n| len = n).is_err() || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        t.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut buf, None).ok()?;
        Some(buf)
    }
}

fn set_size(t: &IMFMediaType, key: &windows::core::GUID, w: u32, h: u32) -> Result<()> {
    unsafe { t.SetUINT64(key, ((w as u64) << 32) | h as u64)? };
    Ok(())
}

fn set_ratio(t: &IMFMediaType, key: &windows::core::GUID, num: u32, den: u32) -> Result<()> {
    unsafe { t.SetUINT64(key, ((num as u64) << 32) | den as u64)? };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::D3dDevice;

    #[test]
    #[ignore = "requires NVENC/HEVC-capable GPU; run with --ignored"]
    fn hardware_hevc_encoder_is_available_and_configurable() {
        crate::win::init_mta();
        crate::win::init_mf();
        let d3d = D3dDevice::create().unwrap();
        let cfg = VideoEncoderConfig {
            width: 1920,
            height: 1080,
            fps_num: 60,
            fps_den: 1,
            bitrate_bps: 20_000_000,
            gop: 120,
            cbr: true,
        };
        let enc = MfHevcEncoder::new(cfg, &d3d.device)
            .expect("hardware HEVC encoder should initialize on this GPU");
        // codec_private may be empty until first output; just prove setup worked.
        let _ = enc.codec_private();
    }
}
