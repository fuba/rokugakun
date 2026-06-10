//! Recording presets (spec §8). HEVC + AAC fixed for the MVP.

use serde::{Deserialize, Serialize};

/// Hardware encoder backend selection. MVP uses Media Foundation (which maps to
/// NVENC/AMF/QSV under the hood); the enum keeps the door open for direct SDKs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EncoderBackend {
    /// Pick the best available Media Foundation hardware MFT.
    #[default]
    Auto,
    MediaFoundation,
    Nvenc,
    Amf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RateControl {
    #[default]
    Cbr,
    Vbr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoPreset {
    #[serde(default)]
    pub codec: String, // "hevc"
    #[serde(default)]
    pub backend: EncoderBackend,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub bitrate_mbps: i32,
    #[serde(default = "default_keyframe_interval")]
    pub keyframe_interval_sec: i32,
    #[serde(default)]
    pub b_frames: i32,
    #[serde(default)]
    pub rate_control: RateControl,
}

fn default_keyframe_interval() -> i32 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioPreset {
    #[serde(default = "default_audio_codec")]
    pub codec: String, // "aac"
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_channels")]
    pub channels: u16,
    #[serde(default = "default_audio_bitrate")]
    pub bitrate_kbps: i32,
}

fn default_audio_codec() -> String {
    "aac".into()
}
fn default_sample_rate() -> u32 {
    48_000
}
fn default_channels() -> u16 {
    2
}
fn default_audio_bitrate() -> i32 {
    192
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SegmentPreset {
    pub max_size_mb: i64,
    pub max_duration_sec: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RetentionPreset {
    pub max_total_gb: i64,
}

/// A full recording preset as stored in `presets/*.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingPreset {
    pub id: String,
    pub name: String,
    pub video: VideoPreset,
    pub audio: AudioPreset,
    pub segment: SegmentPreset,
    pub retention: RetentionPreset,
}

impl RecordingPreset {
    /// Segment size threshold in bytes.
    pub fn segment_max_bytes(&self) -> i64 {
        self.segment.max_size_mb * 1024 * 1024
    }

    /// Retention cap in bytes.
    pub fn retention_max_bytes(&self) -> i64 {
        self.retention.max_total_gb * 1024 * 1024 * 1024
    }

    /// Keyframe spacing in frames (GOP size) at the preset's fps.
    pub fn gop_frames(&self) -> i32 {
        self.video.fps.max(1) * self.video.keyframe_interval_sec.max(1)
    }

    /// The default 1440p60 HEVC preset from spec §8.
    pub fn default_1440p60() -> Self {
        RecordingPreset {
            id: "hevc_1440p60_high".into(),
            name: "HEVC 1440p60 High".into(),
            video: VideoPreset {
                codec: "hevc".into(),
                backend: EncoderBackend::Auto,
                width: 2560,
                height: 1440,
                fps: 60,
                bitrate_mbps: 35,
                keyframe_interval_sec: 2,
                b_frames: 0,
                rate_control: RateControl::Cbr,
            },
            audio: AudioPreset {
                codec: "aac".into(),
                sample_rate: 48_000,
                channels: 2,
                bitrate_kbps: 192,
            },
            segment: SegmentPreset {
                max_size_mb: 1024,
                max_duration_sec: 600,
            },
            retention: RetentionPreset { max_total_gb: 300 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preset_roundtrips_json() {
        let p = RecordingPreset::default_1440p60();
        let json = serde_json::to_string_pretty(&p).unwrap();
        let back: RecordingPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, p.id);
        assert_eq!(back.segment_max_bytes(), 1024 * 1024 * 1024);
        assert_eq!(back.retention_max_bytes(), 300i64 * 1024 * 1024 * 1024);
        assert_eq!(back.gop_frames(), 120);
    }
}
