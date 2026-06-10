//! AAC-LC encoder via the Media Foundation AAC encoder MFT (spec §13).
//!
//! A synchronous MFT: PCM16 in, raw AAC frames out (ADTS is added by the muxer).

use super::{AudioEncoder, AudioEncoderConfig};
use crate::audio::{f32_to_i16, AudioFrame};
use crate::mux::{EncodedPacket, StreamKind};
use anyhow::{Context, Result};
use anyhow::anyhow;
use rec_core::timebase::ns100_to_pts90k;
use std::mem::ManuallyDrop;
use windows::Win32::Media::MediaFoundation::*;

const SAMPLE_RATES: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

pub struct MfAacEncoder {
    transform: IMFTransform,
    asc: [u8; 2],
}
// SAFETY: owned + driven by a single audio-encode thread.
unsafe impl Send for MfAacEncoder {}

impl MfAacEncoder {
    pub fn new(cfg: AudioEncoderConfig) -> Result<Self> {
        crate::win::init_mf();

        let transform = enumerate_aac_encoder()?;

        // Input: PCM16.
        unsafe {
            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            input.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, cfg.sample_rate)?;
            input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, cfg.channels as u32)?;
            transform.SetInputType(0, &input, 0).context("AAC SetInputType")?;
        }

        // Output: AAC.
        unsafe {
            let output = MFCreateMediaType()?;
            output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            output.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
            output.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            output.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, cfg.sample_rate)?;
            output.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, cfg.channels as u32)?;
            output.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, cfg.bitrate_bps / 8)?;
            output.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?; // raw AAC
            transform.SetOutputType(0, &output, 0).context("AAC SetOutputType")?;
        }

        unsafe {
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
        }

        Ok(MfAacEncoder {
            transform,
            asc: make_asc(cfg.sample_rate, cfg.channels),
        })
    }

    fn drain(&mut self, out: &mut Vec<EncodedPacket>) -> Result<()> {
        unsafe {
            let info = self.transform.GetOutputStreamInfo(0)?;
            let out_size = info.cbSize.max(8192);
            loop {
                let buffer = MFCreateMemoryBuffer(out_size)?;
                let sample = MFCreateSample()?;
                sample.AddBuffer(&buffer)?;

                let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                    dwStreamID: 0,
                    pSample: ManuallyDrop::new(Some(sample)),
                    dwStatus: 0,
                    pEvents: ManuallyDrop::new(None),
                }];
                let mut status = 0u32;

                match self.transform.ProcessOutput(0, &mut buffers, &mut status) {
                    Ok(()) => {}
                    Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
                        let _ = ManuallyDrop::into_inner(std::mem::replace(
                            &mut buffers[0].pSample,
                            ManuallyDrop::new(None),
                        ));
                        break;
                    }
                    Err(e) => return Err(e.into()),
                }

                let sample = ManuallyDrop::into_inner(std::mem::replace(
                    &mut buffers[0].pSample,
                    ManuallyDrop::new(None),
                ));
                let Some(sample) = sample else { break };

                let time_100ns = sample.GetSampleTime().unwrap_or(0);
                let media_buffer = sample.ConvertToContiguousBuffer()?;
                let mut ptr = std::ptr::null_mut();
                let mut len = 0u32;
                media_buffer.Lock(&mut ptr, None, Some(&mut len))?;
                let data = std::slice::from_raw_parts(ptr, len as usize).to_vec();
                media_buffer.Unlock()?;

                if !data.is_empty() {
                    let pts_90k = ns100_to_pts90k(time_100ns);
                    out.push(EncodedPacket {
                        data,
                        pts_90k,
                        dts_90k: pts_90k,
                        keyframe: true, // every AAC frame is independently decodable
                        kind: StreamKind::Audio,
                    });
                }
            }
        }
        Ok(())
    }
}

impl AudioEncoder for MfAacEncoder {
    fn encode(&mut self, frame: AudioFrame, out: &mut Vec<EncodedPacket>) -> Result<()> {
        let pcm = f32_to_i16(&frame.samples);
        if pcm.is_empty() {
            return Ok(());
        }
        unsafe {
            let buffer = MFCreateMemoryBuffer(pcm.len() as u32)?;
            let mut ptr = std::ptr::null_mut();
            let mut max = 0u32;
            buffer.Lock(&mut ptr, Some(&mut max), None)?;
            std::ptr::copy_nonoverlapping(pcm.as_ptr(), ptr, pcm.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(pcm.len() as u32)?;

            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(frame.time_100ns)?;
            self.transform.ProcessInput(0, &sample, 0).context("AAC ProcessInput")?;
        }
        self.drain(out)
    }

    fn flush(&mut self, out: &mut Vec<EncodedPacket>) -> Result<()> {
        unsafe {
            self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)?;
        }
        self.drain(out)?;
        Ok(())
    }

    fn asc(&self) -> Vec<u8> {
        self.asc.to_vec()
    }
}

fn enumerate_aac_encoder() -> Result<IMFTransform> {
    let output_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Audio,
        guidSubtype: MFAudioFormat_AAC,
    };
    unsafe {
        let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        MFTEnumEx(
            MFT_CATEGORY_AUDIO_ENCODER,
            MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&output_info),
            &mut activates,
            &mut count,
        )?;
        if count == 0 || activates.is_null() {
            return Err(anyhow!("no AAC encoder MFT found"));
        }
        let slice = std::slice::from_raw_parts(activates, count as usize);
        let activate = slice[0].clone().ok_or_else(|| anyhow!("null IMFActivate"))?;
        let transform: IMFTransform = activate.ActivateObject()?;
        windows::Win32::System::Com::CoTaskMemFree(Some(activates as *const _));
        Ok(transform)
    }
}

fn make_asc(sample_rate: u32, channels: u16) -> [u8; 2] {
    let sr_index = SAMPLE_RATES.iter().position(|&r| r == sample_rate).unwrap_or(3) as u16;
    let obj = 2u16; // AAC-LC
    let v = (obj << 11) | (sr_index << 7) | ((channels & 0x0F) << 3);
    v.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires Media Foundation AAC encoder; run with --ignored"]
    fn encodes_sine_wave_to_aac() {
        crate::win::init_mf();
        let cfg = AudioEncoderConfig {
            sample_rate: 48_000,
            channels: 2,
            bitrate_bps: 192_000,
        };
        let mut enc = MfAacEncoder::new(cfg).expect("create AAC encoder");

        let mut out = Vec::new();
        // ~0.5s of a 440Hz tone in 10ms chunks.
        let mut t = 0i64;
        for chunk in 0..50 {
            let mut samples = Vec::new();
            for i in 0..480 {
                let n = chunk * 480 + i;
                let s = (2.0 * std::f32::consts::PI * 440.0 * n as f32 / 48_000.0).sin() * 0.3;
                samples.push(s);
                samples.push(s);
            }
            enc.encode(
                AudioFrame { samples, sample_rate: 48_000, channels: 2, time_100ns: t },
                &mut out,
            )
            .unwrap();
            t += 100_000; // 10ms in 100ns
        }
        enc.flush(&mut out).unwrap();

        assert!(!out.is_empty(), "AAC encoder produced no frames");
        assert!(out.iter().all(|p| !p.data.is_empty()));
        assert_eq!(enc.asc(), vec![0x11, 0x90]); // AAC-LC 48kHz stereo
    }
}
