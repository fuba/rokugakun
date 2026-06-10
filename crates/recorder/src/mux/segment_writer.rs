//! Segment writer (spec §15).
//!
//! Receives ordered [`EncodedPacket`]s and writes `.ts` segments, rotating only
//! on a video keyframe once a size or duration threshold is crossed (spec §14).
//! Each segment is written to `*.ts.writing`, fsync'd, then renamed into place so
//! a crash leaves a recoverable `.writing` file (spec §16).

use super::{EncodedPacket, StreamKind};
use rec_core::fsutil;
use rec_core::store::Store;
use rec_core::timebase::PTS_HZ;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use ts_mux::{StreamConfig, TsMuxer};

/// Rotation thresholds + identity for a session's segments.
#[derive(Debug, Clone)]
pub struct SegmentParams {
    pub session_id: String,
    pub game_slug: String,
    /// Stamp used in file names, e.g. `20260608_221500`.
    pub session_start: String,
    pub output_dir: PathBuf,
    pub max_size_bytes: i64,
    pub max_duration_sec: i64,
}

/// Info about a segment that was just finalised (drives the `segment_closed` IPC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedSegment {
    pub id: i64,
    pub index_no: i64,
    pub path: PathBuf,
    pub size_bytes: i64,
}

struct OpenSegment {
    id: i64,
    index_no: i64,
    writer: BufWriter<File>,
    tmp_path: PathBuf,
    final_path: PathBuf,
    started_at_ms: i64,
    start_pts: i64,
    last_pts: i64,
    bytes: i64,
}

/// Writes rotating MPEG-TS segments backed by the SQLite manifest.
pub struct SegmentWriter<'a> {
    store: &'a Store,
    muxer: TsMuxer,
    params: SegmentParams,
    next_index: i64,
    cur: Option<OpenSegment>,
    closed: Vec<ClosedSegment>,
}

impl<'a> SegmentWriter<'a> {
    pub fn new(store: &'a Store, stream_cfg: StreamConfig, params: SegmentParams) -> Self {
        SegmentWriter {
            store,
            muxer: TsMuxer::new(stream_cfg),
            params,
            next_index: 1,
            cur: None,
            closed: Vec::new(),
        }
    }

    /// The id of the segment currently being written (excluded from retention).
    pub fn active_segment_id(&self) -> Option<i64> {
        self.cur.as_ref().map(|s| s.id)
    }

    /// Drain segments finalised since the last call (for `segment_closed` IPC).
    pub fn drain_closed(&mut self) -> Vec<ClosedSegment> {
        std::mem::take(&mut self.closed)
    }

    /// Feed one ordered packet. Recording starts at the first video keyframe;
    /// audio arriving before that is dropped (no playable segment yet).
    pub fn write_packet(&mut self, pkt: &EncodedPacket) -> rec_core::Result<()> {
        let is_video = pkt.kind == StreamKind::Video;

        if is_video && pkt.keyframe {
            match self.cur {
                None => self.open_new(pkt.pts_90k)?,
                Some(_) if self.should_rotate() => {
                    self.close_current()?;
                    self.open_new(pkt.pts_90k)?;
                }
                Some(_) => {}
            }
        }

        let Some(seg) = self.cur.as_mut() else {
            // still waiting for the first keyframe
            return Ok(());
        };

        // Mux into a scratch buffer so we can measure bytes + map io errors to path.
        let mut scratch: Vec<u8> = Vec::new();
        match pkt.kind {
            StreamKind::Video => self.muxer.write_video(&mut scratch, &pkt.as_ts()),
            StreamKind::Audio => self.muxer.write_audio(&mut scratch, &pkt.as_ts()),
        }
        .expect("writing to Vec is infallible");

        seg.writer
            .write_all(&scratch)
            .map_err(|e| rec_core::Error::io(&seg.tmp_path, e))?;
        seg.bytes += scratch.len() as i64;
        seg.last_pts = seg.last_pts.max(pkt.pts_90k);
        Ok(())
    }

    /// Close the final segment and flush DB state. Call at session stop.
    pub fn finish(&mut self) -> rec_core::Result<()> {
        if self.cur.is_some() {
            self.close_current()?;
        }
        Ok(())
    }

    fn should_rotate(&self) -> bool {
        let Some(seg) = self.cur.as_ref() else {
            return false;
        };
        let dur_sec = (seg.last_pts - seg.start_pts).max(0) / PTS_HZ;
        seg.bytes >= self.params.max_size_bytes || dur_sec >= self.params.max_duration_sec
    }

    fn open_new(&mut self, start_pts: i64) -> rec_core::Result<()> {
        let index_no = self.next_index;
        self.next_index += 1;

        let file_name = format!(
            "{}_{}_{:06}.ts",
            self.params.game_slug, self.params.session_start, index_no
        );
        let final_path = self.params.output_dir.join(&file_name);
        let tmp_path = self.params.output_dir.join(format!("{file_name}.writing"));

        std::fs::create_dir_all(&self.params.output_dir)
            .map_err(|e| rec_core::Error::io(&self.params.output_dir, e))?;
        let file = File::create(&tmp_path).map_err(|e| rec_core::Error::io(&tmp_path, e))?;
        let mut writer = BufWriter::new(file);

        let started_at_ms = fsutil::now_ms();
        let id = self.store.insert_segment(
            &self.params.session_id,
            &final_path.to_string_lossy(),
            index_no,
            started_at_ms,
            Some(start_pts),
        )?;

        // Each segment opens with its own PAT/PMT so it is independently playable.
        let mut header: Vec<u8> = Vec::new();
        self.muxer
            .begin_segment(&mut header)
            .expect("writing to Vec is infallible");
        writer
            .write_all(&header)
            .map_err(|e| rec_core::Error::io(&tmp_path, e))?;

        self.cur = Some(OpenSegment {
            id,
            index_no,
            writer,
            tmp_path,
            final_path,
            started_at_ms,
            start_pts,
            last_pts: start_pts,
            bytes: header.len() as i64,
        });
        Ok(())
    }

    fn close_current(&mut self) -> rec_core::Result<()> {
        let Some(mut seg) = self.cur.take() else {
            return Ok(());
        };

        seg.writer
            .flush()
            .map_err(|e| rec_core::Error::io(&seg.tmp_path, e))?;
        let file = seg
            .writer
            .into_inner()
            .map_err(|e| rec_core::Error::io(&seg.tmp_path, e.into_error()))?;
        file.sync_all().map_err(|e| rec_core::Error::io(&seg.tmp_path, e))?;
        drop(file);

        std::fs::rename(&seg.tmp_path, &seg.final_path)
            .map_err(|e| rec_core::Error::io(&seg.final_path, e))?;

        let ended_at_ms = fsutil::now_ms();
        let duration_ms = ((seg.last_pts - seg.start_pts).max(0)) * 1000 / PTS_HZ;
        self.store
            .close_segment(seg.id, ended_at_ms, duration_ms, seg.bytes, Some(seg.last_pts))?;

        let _ = seg.started_at_ms;
        self.closed.push(ClosedSegment {
            id: seg.id,
            index_no: seg.index_no,
            path: seg.final_path,
            size_bytes: seg.bytes,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rec_core::domain::{Game, Session, SessionStatus};

    fn stream_cfg() -> StreamConfig {
        StreamConfig {
            hevc_vps_sps_pps: vec![0, 0, 0, 1, 0x40, 0x01],
            aac_sample_rate: 48_000,
            aac_channels: 2,
        }
    }

    fn seeded_store() -> Store {
        let s = Store::open_in_memory().unwrap();
        s.upsert_game(&Game {
            id: "g1".into(),
            name: "Forza".into(),
            launch_command: "f.exe".into(),
            launch_workdir: None,
            launch_args: None,
            auto_record: false,
            preset: None,
            created_at: 0,
            updated_at: 0,
        })
        .unwrap();
        s.insert_session(&Session {
            id: "sess".into(),
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
        s
    }

    fn params(dir: PathBuf, max_bytes: i64, max_sec: i64) -> SegmentParams {
        SegmentParams {
            session_id: "sess".into(),
            game_slug: "Forza".into(),
            session_start: "20260608_221500".into(),
            output_dir: dir,
            max_size_bytes: max_bytes,
            max_duration_sec: max_sec,
        }
    }

    fn vkey(pts: i64) -> EncodedPacket {
        EncodedPacket { data: vec![0u8; 200], pts_90k: pts, dts_90k: pts, keyframe: true, kind: StreamKind::Video }
    }
    fn vdelta(pts: i64) -> EncodedPacket {
        EncodedPacket { data: vec![0u8; 200], pts_90k: pts, dts_90k: pts, keyframe: false, kind: StreamKind::Video }
    }

    #[test]
    fn first_keyframe_opens_segment_audio_before_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store();
        let mut w = SegmentWriter::new(&store, stream_cfg(), params(dir.path().into(), 1 << 30, 600));

        // audio before any keyframe -> dropped, no segment yet
        let audio = EncodedPacket { data: vec![1, 2, 3], pts_90k: 0, dts_90k: 0, keyframe: true, kind: StreamKind::Audio };
        w.write_packet(&audio).unwrap();
        assert!(w.active_segment_id().is_none());

        w.write_packet(&vkey(0)).unwrap();
        assert!(w.active_segment_id().is_some());
        w.finish().unwrap();

        let segs = store.list_segments("sess").unwrap();
        assert_eq!(segs.len(), 1);
        assert!(segs[0].closed);
        // file renamed into place, no .writing left
        assert!(PathBuf::from(&segs[0].path).exists());
        assert!(!PathBuf::from(format!("{}.writing", segs[0].path)).exists());
    }

    #[test]
    fn rotates_on_keyframe_after_size_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store();
        // tiny cap so the second keyframe triggers rotation
        let mut w = SegmentWriter::new(&store, stream_cfg(), params(dir.path().into(), 300, 600));

        w.write_packet(&vkey(0)).unwrap(); // opens seg 1 (header + 200 bytes > 300? header ~ a few packets)
        w.write_packet(&vdelta(1500)).unwrap();
        // by now seg1 exceeds 300 bytes; next keyframe should rotate
        w.write_packet(&vkey(90_000)).unwrap();
        w.write_packet(&vdelta(91_500)).unwrap();
        w.finish().unwrap();

        let segs = store.list_segments("sess").unwrap();
        assert_eq!(segs.len(), 2, "expected a rotation into a second segment");
        assert!(segs.iter().all(|s| s.closed));
        assert_eq!(segs[0].index_no, 1);
        assert_eq!(segs[1].index_no, 2);
        // file names follow {slug}_{start}_{index}.ts
        assert!(segs[1].path.ends_with("Forza_20260608_221500_000002.ts"));
    }

    #[test]
    fn rotates_on_duration_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store();
        // 1 second max duration, huge size cap
        let mut w = SegmentWriter::new(&store, stream_cfg(), params(dir.path().into(), 1 << 30, 1));

        w.write_packet(&vkey(0)).unwrap();
        w.write_packet(&vdelta(90_000)).unwrap(); // advances last_pts to 1s
        w.write_packet(&vkey(180_000)).unwrap(); // 2s mark -> rotate
        w.finish().unwrap();

        assert_eq!(store.list_segments("sess").unwrap().len(), 2);
    }

    #[test]
    fn no_keyframe_means_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store();
        let mut w = SegmentWriter::new(&store, stream_cfg(), params(dir.path().into(), 1 << 30, 600));
        w.write_packet(&vdelta(0)).unwrap(); // delta with no prior keyframe -> dropped
        w.finish().unwrap();
        assert_eq!(store.list_segments("sess").unwrap().len(), 0);
    }

    #[test]
    fn drain_closed_reports_segments() {
        let dir = tempfile::tempdir().unwrap();
        let store = seeded_store();
        let mut w = SegmentWriter::new(&store, stream_cfg(), params(dir.path().into(), 300, 600));
        w.write_packet(&vkey(0)).unwrap();
        w.write_packet(&vdelta(1500)).unwrap();
        w.write_packet(&vkey(90_000)).unwrap();
        let closed = w.drain_closed();
        assert_eq!(closed.len(), 1); // seg 1 closed when seg 2 opened
        assert_eq!(closed[0].index_no, 1);
        assert!(closed[0].size_bytes > 0);
        w.finish().unwrap();
        assert_eq!(w.drain_closed().len(), 1); // seg 2 closed on finish
    }
}
