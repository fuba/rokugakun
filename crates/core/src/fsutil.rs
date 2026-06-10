//! Small filesystem + clock helpers shared across the crate.

use crate::error::{Error, Result};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current wall-clock time as Unix epoch milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// File size in bytes, or an io error tagged with the path.
pub fn file_size(path: &Path) -> Result<i64> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    Ok(meta.len() as i64)
}

/// Delete a file. Treats "already gone" as success (retention is idempotent).
pub fn remove_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io(path, e)),
    }
}

/// Filesystem-safe slug for a game name (used in segment file names, spec §15).
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if ch == ' ' || ch == '-' || ch == '_' {
            // collapse separators into a single underscore
            if !out.ends_with('_') {
                out.push('_');
            }
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "game".into()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_examples() {
        assert_eq!(slugify("Forza Horizon 5"), "Forza_Horizon_5");
        assert_eq!(slugify("Starfield"), "Starfield");
        assert_eq!(slugify("  !!!  "), "game");
        assert_eq!(slugify("a---b"), "a_b");
    }

    #[test]
    fn now_is_positive() {
        assert!(now_ms() > 1_700_000_000_000);
    }
}
