//! App configuration and directory layout (spec §25).
//!
//! ```text
//! %LOCALAPPDATA%\GameRecorder\{config.json, recorder.db, logs\, presets\, temp\}
//! D:\GameRecordings\<game>\<session>\seg_xxxxxx.ts   (+ session.json)
//! ```

use crate::error::{Error, Result};
use crate::preset::RecordingPreset;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Resolved on-disk locations for app data (not the recordings themselves).
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_path: PathBuf,
    pub db_path: PathBuf,
    pub logs_dir: PathBuf,
    pub presets_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl AppPaths {
    /// Layout rooted at `%LOCALAPPDATA%\GameRecorder` (falls back to `./.gamerecorder`
    /// on hosts without `LOCALAPPDATA`, e.g. CI).
    pub fn default_layout() -> Self {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".").join(".local"));
        Self::rooted_at(base.join("GameRecorder"))
    }

    /// Layout rooted at an explicit directory (used by tests).
    pub fn rooted_at(data_dir: PathBuf) -> Self {
        AppPaths {
            config_path: data_dir.join("config.json"),
            db_path: data_dir.join("recorder.db"),
            logs_dir: data_dir.join("logs"),
            presets_dir: data_dir.join("presets"),
            temp_dir: data_dir.join("temp"),
            data_dir,
        }
    }

    /// Create every app-data directory if missing.
    pub fn ensure_dirs(&self) -> Result<()> {
        for d in [&self.data_dir, &self.logs_dir, &self.presets_dir, &self.temp_dir] {
            std::fs::create_dir_all(d).map_err(|e| Error::io(d, e))?;
        }
        Ok(())
    }
}

/// User-facing configuration persisted to `config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Where recordings are written (spec §17 storage root).
    pub storage_root: PathBuf,
    /// Preset id used for new games unless overridden.
    pub default_preset_id: String,
    /// The active recording preset (all quality/segment/retention settings).
    #[serde(default = "default_preset")]
    pub preset: RecordingPreset,
}

fn default_preset() -> RecordingPreset {
    RecordingPreset::default_1440p60()
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            storage_root: default_storage_root(),
            default_preset_id: "hevc_1440p60_high".into(),
            preset: default_preset(),
        }
    }
}

/// A storage root that exists out of the box: `%USERPROFILE%\Videos\GameRecordings`.
pub fn default_storage_root() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| p.join("Videos").join("GameRecordings"))
        .unwrap_or_else(|| PathBuf::from("GameRecordings"))
}

impl AppConfig {
    /// Load from `path`, or return defaults if the file does not exist.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(serde_json::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::io(path, e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(path, s).map_err(|e| Error::io(path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::rooted_at(dir.path().join("GameRecorder"));
        paths.ensure_dirs().unwrap();
        assert!(paths.presets_dir.is_dir());

        let cfg = AppConfig::default();
        cfg.save(&paths.config_path).unwrap();
        let back = AppConfig::load_or_default(&paths.config_path).unwrap();
        assert_eq!(back.default_preset_id, cfg.default_preset_id);
    }

    #[test]
    fn missing_config_is_default() {
        let cfg = AppConfig::load_or_default(Path::new("/no/such/config.json")).unwrap();
        assert_eq!(cfg.default_preset_id, "hevc_1440p60_high");
    }
}
