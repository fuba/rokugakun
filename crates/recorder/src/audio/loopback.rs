//! Per-process WASAPI loopback capture (spec §10).
//!
//! Activates the virtual `VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK` device targeting
//! a PID's process tree via `ActivateAudioInterfaceAsync` (async, so we bridge to
//! a sync `IAudioClient` with a COM completion handler + event). Capture is float32
//! at the requested rate; `poll` drains available packets into [`AudioFrame`]s.

use super::AudioFrame;
use anyhow::{anyhow, Result};
use windows::core::{implement, Interface, GUID};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};

/// PROPVARIANT laid out for a `VT_BLOB` (avoids the deeply-nested union wrapper).
/// x64 PROPVARIANT is 24 bytes: 8 header + 16 union; BLOB = {cbSize:u32, pad, ptr}.
#[repr(C)]
struct BlobPropVariant {
    vt: u16,
    r1: u16,
    r2: u16,
    r3: u16,
    cb_size: u32,
    _pad: u32,
    blob_ptr: *mut core::ffi::c_void,
}
const VT_BLOB: u16 = 65;

/// Completion handler that just signals an event when activation finishes.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationHandler {
    event: HANDLE,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationHandler_Impl {
    fn ActivateCompleted(
        &self,
        _op: Option<&IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        unsafe {
            let _ = SetEvent(self.event);
        }
        Ok(())
    }
}

/// An active per-process loopback capture stream.
pub struct LoopbackCapture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    sample_rate: u32,
    channels: u16,
}
// SAFETY: owned + used by a single capture thread after construction.
unsafe impl Send for LoopbackCapture {}

impl LoopbackCapture {
    /// Activate + initialise + start loopback for `pid`'s process tree.
    pub fn for_process(pid: u32, sample_rate: u32, channels: u16) -> Result<Self> {
        crate::win::init_mta();

        let client = activate_process_loopback(pid)?;

        let block_align = channels * 4; // float32
        let format = WAVEFORMATEX {
            wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
            nChannels: channels,
            nSamplesPerSec: sample_rate,
            nAvgBytesPerSec: sample_rate * block_align as u32,
            nBlockAlign: block_align,
            wBitsPerSample: 32,
            cbSize: 0,
        };

        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                2_000_000, // 200ms buffer (REFERENCE_TIME, 100ns units)
                0,
                &format,
                None,
            )?;
        }

        let capture: IAudioCaptureClient = unsafe { client.GetService()? };
        unsafe { client.Start()? };

        Ok(LoopbackCapture {
            client,
            capture,
            sample_rate,
            channels,
        })
    }

    /// Drain all currently-available audio packets into `out`.
    pub fn poll(&self, out: &mut Vec<AudioFrame>) -> Result<()> {
        unsafe {
            loop {
                let packet = self.capture.GetNextPacketSize()?;
                if packet == 0 {
                    break;
                }
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut num_frames = 0u32;
                let mut flags = 0u32;
                let mut qpc_pos = 0u64;
                self.capture.GetBuffer(
                    &mut data,
                    &mut num_frames,
                    &mut flags,
                    None,
                    Some(&mut qpc_pos),
                )?;

                let sample_count = num_frames as usize * self.channels as usize;
                let samples = if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 {
                    // silent buffer: emit zeros so PTS stays continuous (spec §11)
                    vec![0.0f32; sample_count]
                } else {
                    std::slice::from_raw_parts(data as *const f32, sample_count).to_vec()
                };

                out.push(AudioFrame {
                    samples,
                    sample_rate: self.sample_rate,
                    channels: self.channels,
                    time_100ns: qpc_pos as i64,
                });

                self.capture.ReleaseBuffer(num_frames)?;
            }
        }
        Ok(())
    }

    pub fn stop(&self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

impl Drop for LoopbackCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Drive `ActivateAudioInterfaceAsync` to completion and return the IAudioClient.
fn activate_process_loopback(pid: u32) -> Result<IAudioClient> {
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };

    let prop = BlobPropVariant {
        vt: VT_BLOB,
        r1: 0,
        r2: 0,
        r3: 0,
        cb_size: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
        _pad: 0,
        blob_ptr: &mut params as *mut _ as *mut core::ffi::c_void,
    };

    unsafe {
        let event = CreateEventW(None, false, false, None)?;
        let handler: IActivateAudioInterfaceCompletionHandler =
            ActivationHandler { event }.into();

        let op = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID as *const GUID,
            Some(&prop as *const _ as *const _),
            &handler,
        )?;

        if WaitForSingleObject(event, 5_000) != WAIT_OBJECT_0 {
            return Err(anyhow!("audio activation timed out"));
        }

        let mut hr = windows::core::HRESULT(0);
        let mut iface: Option<windows::core::IUnknown> = None;
        op.GetActivateResult(&mut hr, &mut iface)?;
        hr.ok()?;
        let unknown = iface.ok_or_else(|| anyhow!("null activated interface"))?;
        Ok(unknown.cast()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "activates WASAPI process loopback; run with --ignored"]
    fn activate_own_process_loopback() {
        // Targeting our own (likely silent) PID still validates the activation +
        // Initialize + Start path; we just confirm no error and poll briefly.
        let cap = LoopbackCapture::for_process(std::process::id(), 48_000, 2)
            .expect("process loopback should activate");
        let mut frames = Vec::new();
        for _ in 0..5 {
            cap.poll(&mut frames).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        cap.stop();
    }
}
