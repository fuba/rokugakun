//! Domain types mirroring the SQLite manifest (spec §6/§7).
//!
//! All `*_at` timestamps are Unix epoch **milliseconds** (i64).

use crate::preset::RecordingPreset;
use serde::{Deserialize, Serialize};

pub type GameId = String;
pub type SessionId = String;

/// A registered game (spec §6 `games`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub name: String,
    /// exe path / .lnk / `shell:AppsFolder\<AUMID>` / URI / arbitrary command.
    pub launch_command: String,
    pub launch_workdir: Option<String>,
    pub launch_args: Option<String>,
    /// Auto-start recording when this game's process is detected running.
    #[serde(default)]
    pub auto_record: bool,
    /// Per-game quality override; `None` uses the global preset.
    #[serde(default)]
    pub preset: Option<RecordingPreset>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Window identification rule (spec §6 `game_window_rules`).
///
/// Match priority: AUMID > image path > process name > class > title regex.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowRule {
    pub id: Option<i64>,
    pub game_id: GameId,
    pub exe_path: Option<String>,
    pub process_name: Option<String>,
    pub window_title_pattern: Option<String>,
    pub window_class: Option<String>,
    pub app_user_model_id: Option<String>,
    pub preferred_monitor_index: Option<i32>,
    pub last_hwnd: Option<i64>,
    pub confidence: i32,
}

/// Lifecycle of a recording session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Recording,
    Stopped,
    Error,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Recording => "recording",
            SessionStatus::Stopped => "stopped",
            SessionStatus::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "recording" => SessionStatus::Recording,
            "error" => SessionStatus::Error,
            _ => SessionStatus::Stopped,
        }
    }
}

/// A recording session (spec §7 `sessions`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub game_id: GameId,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub codec_video: String,
    pub codec_audio: String,
    pub container: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps_num: Option<i32>,
    pub fps_den: Option<i32>,
    pub bitrate_video: Option<i32>,
    pub bitrate_audio: Option<i32>,
    pub storage_root: String,
    pub status: SessionStatus,
}

/// A single MPEG-TS segment row (spec §7/§16 `segments`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: i64,
    pub session_id: SessionId,
    pub path: String,
    pub index_no: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub size_bytes: i64,
    pub start_pts: Option<i64>,
    pub end_pts: Option<i64>,
    pub protected: bool,
    pub closed: bool,
    pub deleted: bool,
    pub deleting: bool,
}
