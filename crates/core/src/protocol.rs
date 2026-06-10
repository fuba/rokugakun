//! IPC protocol between launcher and recorder (spec §4 / §31).
//!
//! Transport is a named pipe carrying newline-delimited JSON (JSON Lines). These
//! types are the wire contract; the transport itself lives in the bin crates.

use serde::{Deserialize, Serialize};

/// Named pipe address (spec §4).
pub const PIPE_NAME: &str = r"\\.\pipe\game-recorder-control";

/// Messages launcher -> recorder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LauncherMsg {
    /// Spin up the pipeline in pending mode before the game launches.
    Prepare {
        session_id: String,
        preset_id: String,
        output_dir: String,
    },
    /// Target window found; start Windows.Graphics.Capture.
    AttachWindow { hwnd: i64 },
    /// Resolve the audio capture target.
    SetAudioProcess { pid: u32, include_tree: bool },
    /// Begin writing media (first frame onwards).
    Start,
    /// Stop recording and flush.
    Stop,
    /// Mark a segment protected from retention.
    MarkProtected { segment_id: i64 },
    /// Ask the recorder for a status frame.
    Status,
}

/// Messages recorder -> launcher.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecorderMsg {
    /// Current state and live counters.
    Status {
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        size_bytes: Option<i64>,
    },
    /// A segment finished and was renamed into place.
    SegmentClosed { path: String, size_bytes: i64 },
    /// A fatal or recoverable error (e.g. `AUDIO_TARGET_NOT_FOUND`).
    Error { code: String, message: String },
}

impl LauncherMsg {
    /// Serialize as a single JSON Lines record (with trailing `\n`).
    pub fn to_line(&self) -> serde_json::Result<String> {
        Ok(format!("{}\n", serde_json::to_string(self)?))
    }
}

impl RecorderMsg {
    pub fn to_line(&self) -> serde_json::Result<String> {
        Ok(format!("{}\n", serde_json::to_string(self)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_msg_tagged_json() {
        let m = LauncherMsg::AttachWindow { hwnd: 123456 };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, r#"{"type":"attach_window","hwnd":123456}"#);
        let back: LauncherMsg = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn prepare_roundtrip() {
        let m = LauncherMsg::Prepare {
            session_id: "s1".into(),
            preset_id: "hevc_1440p60_high".into(),
            output_dir: r"D:\GameRecordings\Forza\20260608".into(),
        };
        let line = m.to_line().unwrap();
        assert!(line.ends_with('\n'));
        let back: LauncherMsg = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn recorder_status_omits_none() {
        let m = RecorderMsg::Status {
            state: "waiting_for_window".into(),
            duration_ms: None,
            size_bytes: None,
        };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, r#"{"type":"status","state":"waiting_for_window"}"#);
    }

    #[test]
    fn error_msg() {
        let m = RecorderMsg::Error {
            code: "AUDIO_TARGET_NOT_FOUND".into(),
            message: "no audio".into(),
        };
        let back: RecorderMsg = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }
}
