//! Encoder abstractions (spec §12). Media Foundation backends are added in
//! Steps 4–5; the traits keep a future direct-NVENC backend swappable.

use crate::audio::AudioFrame;
use crate::capture::GridFrame;
use crate::mux::EncodedPacket;

#[cfg(windows)]
pub mod aac_mf;
#[cfg(windows)]
pub mod hevc_mf;

/// Audio encoder configuration (AAC-LC).
#[derive(Debug, Clone, Copy)]
pub struct AudioEncoderConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub bitrate_bps: u32,
}

/// Video encoder configuration (HEVC, no B-frames).
#[derive(Debug, Clone, Copy)]
pub struct VideoEncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub bitrate_bps: u32,
    /// Keyframe interval in frames (GOP size).
    pub gop: u32,
    /// true = CBR, false = VBR (rate control mode).
    pub cbr: bool,
    /// Which hardware vendor's encoder MFT to prefer (Auto = first available).
    pub backend: rec_core::preset::EncoderBackend,
}

/// Video encoder producing Annex B HEVC packets (DTS=PTS, no B-frames).
pub trait VideoEncoder: Send {
    /// Encode one grid frame, appending any produced packets to `out`.
    fn encode(&mut self, frame: GridFrame, out: &mut Vec<EncodedPacket>) -> anyhow::Result<()>;
    /// Drain buffered packets at end of stream.
    fn flush(&mut self, out: &mut Vec<EncodedPacket>) -> anyhow::Result<()>;
    /// VPS/SPS/PPS as Annex B, re-emitted at each segment start.
    fn codec_private(&self) -> Vec<u8>;
}

/// Audio encoder producing raw AAC frames (ADTS added by the muxer).
pub trait AudioEncoder: Send {
    fn encode(&mut self, frame: AudioFrame, out: &mut Vec<EncodedPacket>) -> anyhow::Result<()>;
    fn flush(&mut self, out: &mut Vec<EncodedPacket>) -> anyhow::Result<()>;
    /// AudioSpecificConfig (2 bytes for AAC-LC).
    fn asc(&self) -> Vec<u8>;
}
