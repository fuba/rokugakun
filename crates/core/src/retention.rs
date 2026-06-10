//! Capacity rotation (spec §17).
//!
//! Deletes the oldest eligible segments until total size is under the cap. The
//! file delete and DB update can't share a transaction, so we use the ordering:
//! `deleting=1` -> remove file -> `deleted=1` (spec §16). The currently-writing
//! segment is never touched.

use crate::error::Result;
use crate::fsutil;
use crate::store::Store;
use std::path::Path;

/// Runs retention against a [`Store`] + the filesystem.
pub struct RetentionManager<'a> {
    store: &'a Store,
    max_total_bytes: i64,
}

/// What a [`RetentionManager::cleanup`] pass did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CleanupReport {
    pub deleted_ids: Vec<i64>,
    pub freed_bytes: i64,
    /// True if the cap could not be reached (ran out of deletable segments, or a
    /// delete failed).
    pub stalled: bool,
}

impl<'a> RetentionManager<'a> {
    pub fn new(store: &'a Store, max_total_bytes: i64) -> Self {
        Self {
            store,
            max_total_bytes,
        }
    }

    /// Delete oldest deletable segments until under the cap (spec §17 loop).
    ///
    /// `active_segment_id` is the segment currently being written, if any; it is
    /// excluded from deletion.
    pub fn cleanup(&self, active_segment_id: Option<i64>) -> Result<CleanupReport> {
        let mut report = CleanupReport::default();
        let mut total = self.store.total_size()?;

        while total > self.max_total_bytes {
            let Some(seg) = self.store.oldest_deletable(active_segment_id)? else {
                report.stalled = true;
                break;
            };

            self.store.mark_deleting(seg.id)?;
            match fsutil::remove_file(Path::new(&seg.path)) {
                Ok(()) => {
                    self.store.mark_deleted(seg.id)?;
                    total -= seg.size_bytes;
                    report.freed_bytes += seg.size_bytes;
                    report.deleted_ids.push(seg.id);
                    tracing::debug!(id = seg.id, path = %seg.path, "retention deleted segment");
                }
                Err(e) => {
                    // Couldn't delete: undo the flag and stop to avoid a hot loop.
                    self.store.clear_deleting(seg.id)?;
                    tracing::warn!(id = seg.id, error = %e, "retention delete failed");
                    report.stalled = true;
                    break;
                }
            }
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Session, SessionStatus};

    fn seed_session(store: &Store) {
        store
            .upsert_game(&crate::domain::Game {
                id: "g1".into(),
                name: "G".into(),
                launch_command: "g.exe".into(),
                launch_workdir: None,
                launch_args: None,
                auto_record: false,
                preset: None,
                created_at: 0,
                updated_at: 0,
            })
            .unwrap();
        store
            .insert_session(&Session {
                id: "s1".into(),
                game_id: "g1".into(),
                started_at: 0,
                ended_at: None,
                codec_video: "hevc".into(),
                codec_audio: "aac".into(),
                container: "mpegts".into(),
                width: None,
                height: None,
                fps_num: None,
                fps_den: None,
                bitrate_video: None,
                bitrate_audio: None,
                storage_root: "x".into(),
                status: SessionStatus::Recording,
            })
            .unwrap();
    }

    /// Create a real file of `size` bytes and a matching closed segment row.
    fn make_segment(store: &Store, dir: &Path, idx: i64, started: i64, size: usize) -> (i64, std::path::PathBuf) {
        let path = dir.join(format!("seg_{idx:06}.ts"));
        std::fs::write(&path, vec![0u8; size]).unwrap();
        let id = store
            .insert_segment("s1", path.to_str().unwrap(), idx, started, None)
            .unwrap();
        store.close_segment(id, started + 1, 1, size as i64, None).unwrap();
        (id, path)
    }

    #[test]
    fn drops_until_below_cap_and_keeps_newest() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        seed_session(&store);
        let (id1, p1) = make_segment(&store, dir.path(), 1, 100, 100);
        let (id2, p2) = make_segment(&store, dir.path(), 2, 200, 100);
        let (_id3, p3) = make_segment(&store, dir.path(), 3, 300, 100);

        let rm = RetentionManager::new(&store, 150);
        let report = rm.cleanup(None).unwrap();

        // 300 -> delete id1 (200) -> delete id2 (100) -> stop (<=150)
        assert_eq!(report.deleted_ids, vec![id1, id2]);
        assert_eq!(report.freed_bytes, 200);
        assert!(!p1.exists());
        assert!(!p2.exists());
        assert!(p3.exists());
        assert_eq!(store.total_size().unwrap(), 100);
        assert!(!report.stalled);
    }

    #[test]
    fn protected_and_active_are_kept_even_over_cap() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        seed_session(&store);
        let (id1, p1) = make_segment(&store, dir.path(), 1, 100, 100);
        let (id2, p2) = make_segment(&store, dir.path(), 2, 200, 100);
        store.set_protected(id1, true).unwrap();

        // cap 0, but id1 protected and id2 active -> nothing deletable
        let rm = RetentionManager::new(&store, 0);
        let report = rm.cleanup(Some(id2)).unwrap();
        assert!(report.deleted_ids.is_empty());
        assert!(report.stalled);
        assert!(p1.exists());
        assert!(p2.exists());
    }
}
