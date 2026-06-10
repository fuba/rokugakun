//! SQLite manifest (spec §6/§7/§16).
//!
//! The DB is the *index*; the filesystem is the other half of the truth. On open
//! we run migrations and can reconcile the two via [`Store::integrity_scan`].

use crate::domain::*;
use crate::error::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i64 = 4;

/// Owns the SQLite connection to the manifest.
pub struct Store {
    conn: Connection,
}

/// Aggregated info for one recording session (for the viewer).
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session: Session,
    pub segment_count: i64,
    pub total_bytes: i64,
    pub duration_ms: i64,
}

/// A session summary together with its game's name (works for archived games
/// too, so recordings stay reachable after a game is "deleted").
#[derive(Debug, Clone)]
pub struct RecentSession {
    pub game_name: String,
    pub summary: SessionSummary,
}

impl Store {
    /// Open (creating if needed) the manifest at `db_path`, applying migrations.
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::init(conn)
    }

    /// In-memory store for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 =
            self.conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            self.conn.execute_batch(SCHEMA_V1)?;
        }
        if version < 2 {
            self.conn.execute_batch(
                "ALTER TABLE games ADD COLUMN auto_record INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        if version < 3 {
            self.conn
                .execute_batch("ALTER TABLE games ADD COLUMN preset_json TEXT;")?;
        }
        if version < 4 {
            // "Deleting" a game archives it so its sessions stay reachable.
            self.conn.execute_batch(
                "ALTER TABLE games ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        self.conn
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    // ---- games -------------------------------------------------------------

    pub fn upsert_game(&self, g: &Game) -> Result<()> {
        let preset_json: Option<String> =
            g.preset.as_ref().and_then(|p| serde_json::to_string(p).ok());
        self.conn.execute(
            "INSERT INTO games (id,name,launch_command,launch_workdir,launch_args,auto_record,preset_json,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, launch_command=excluded.launch_command,
               launch_workdir=excluded.launch_workdir, launch_args=excluded.launch_args,
               auto_record=excluded.auto_record, preset_json=excluded.preset_json,
               updated_at=excluded.updated_at",
            params![g.id, g.name, g.launch_command, g.launch_workdir, g.launch_args, g.auto_record as i64, preset_json, g.created_at, g.updated_at],
        )?;
        Ok(())
    }

    /// "Delete" a game: archive it (hidden from the games list) and drop its
    /// window rules. Sessions/segments stay so recordings remain watchable.
    pub fn delete_game(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM game_window_rules WHERE game_id=?1", [id])?;
        self.conn
            .execute("UPDATE games SET archived=1 WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn get_game(&self, id: &str) -> Result<Option<Game>> {
        let g = self
            .conn
            .query_row(
                "SELECT id,name,launch_command,launch_workdir,launch_args,auto_record,created_at,updated_at,preset_json
                 FROM games WHERE id=?1",
                [id],
                Self::map_game,
            )
            .optional()?;
        Ok(g)
    }

    pub fn list_games(&self) -> Result<Vec<Game>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,name,launch_command,launch_workdir,launch_args,auto_record,created_at,updated_at,preset_json
             FROM games WHERE archived=0 ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], Self::map_game)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn map_game(r: &rusqlite::Row) -> rusqlite::Result<Game> {
        let preset = r
            .get::<_, Option<String>>(8)?
            .and_then(|s| serde_json::from_str(&s).ok());
        Ok(Game {
            id: r.get(0)?,
            name: r.get(1)?,
            launch_command: r.get(2)?,
            launch_workdir: r.get(3)?,
            launch_args: r.get(4)?,
            auto_record: r.get::<_, i64>(5)? != 0,
            preset,
            created_at: r.get(6)?,
            updated_at: r.get(7)?,
        })
    }

    // ---- window rules ------------------------------------------------------

    /// Insert or replace the window rule for a game. Returns the rule id.
    pub fn upsert_window_rule(&self, r: &WindowRule) -> Result<i64> {
        // One active rule per game in the MVP: delete prior, insert fresh.
        self.conn
            .execute("DELETE FROM game_window_rules WHERE game_id=?1", [&r.game_id])?;
        self.conn.execute(
            "INSERT INTO game_window_rules
               (game_id,exe_path,process_name,window_title_pattern,window_class,
                app_user_model_id,preferred_monitor_index,last_hwnd,confidence)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                r.game_id, r.exe_path, r.process_name, r.window_title_pattern, r.window_class,
                r.app_user_model_id, r.preferred_monitor_index, r.last_hwnd, r.confidence
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_window_rule(&self, game_id: &str) -> Result<Option<WindowRule>> {
        let rule = self
            .conn
            .query_row(
                "SELECT id,game_id,exe_path,process_name,window_title_pattern,window_class,
                        app_user_model_id,preferred_monitor_index,last_hwnd,confidence
                 FROM game_window_rules WHERE game_id=?1",
                [game_id],
                |r| {
                    Ok(WindowRule {
                        id: r.get(0)?,
                        game_id: r.get(1)?,
                        exe_path: r.get(2)?,
                        process_name: r.get(3)?,
                        window_title_pattern: r.get(4)?,
                        window_class: r.get(5)?,
                        app_user_model_id: r.get(6)?,
                        preferred_monitor_index: r.get(7)?,
                        last_hwnd: r.get(8)?,
                        confidence: r.get(9)?,
                    })
                },
            )
            .optional()?;
        Ok(rule)
    }

    // ---- sessions ----------------------------------------------------------

    pub fn insert_session(&self, s: &Session) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions
               (id,game_id,started_at,ended_at,codec_video,codec_audio,container,
                width,height,fps_num,fps_den,bitrate_video,bitrate_audio,storage_root,status)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                s.id, s.game_id, s.started_at, s.ended_at, s.codec_video, s.codec_audio, s.container,
                s.width, s.height, s.fps_num, s.fps_den, s.bitrate_video, s.bitrate_audio,
                s.storage_root, s.status.as_str()
            ],
        )?;
        Ok(())
    }

    pub fn end_session(&self, id: &str, ended_at: i64, status: SessionStatus) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET ended_at=?2, status=?3 WHERE id=?1",
            params![id, ended_at, status.as_str()],
        )?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let s = self
            .conn
            .query_row(
                "SELECT id,game_id,started_at,ended_at,codec_video,codec_audio,container,
                        width,height,fps_num,fps_den,bitrate_video,bitrate_audio,storage_root,status
                 FROM sessions WHERE id=?1",
                [id],
                Self::map_session,
            )
            .optional()?;
        Ok(s)
    }

    fn map_session(r: &rusqlite::Row) -> rusqlite::Result<Session> {
        let status: String = r.get(14)?;
        Ok(Session {
            id: r.get(0)?,
            game_id: r.get(1)?,
            started_at: r.get(2)?,
            ended_at: r.get(3)?,
            codec_video: r.get(4)?,
            codec_audio: r.get(5)?,
            container: r.get(6)?,
            width: r.get(7)?,
            height: r.get(8)?,
            fps_num: r.get(9)?,
            fps_den: r.get(10)?,
            bitrate_video: r.get(11)?,
            bitrate_audio: r.get(12)?,
            storage_root: r.get(13)?,
            status: SessionStatus::parse(&status),
        })
    }

    // ---- viewer queries ----------------------------------------------------

    /// Per-session summaries for a game (newest first); sessions with no
    /// remaining segments are skipped.
    pub fn session_summaries(&self, game_id: &str) -> Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,game_id,started_at,ended_at,codec_video,codec_audio,container,
                    width,height,fps_num,fps_den,bitrate_video,bitrate_audio,storage_root,status
             FROM sessions WHERE game_id=?1 ORDER BY started_at DESC",
        )?;
        let sessions = stmt
            .query_map([game_id], Self::map_session)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut out = Vec::new();
        for session in sessions {
            let (count, bytes, dur): (i64, i64, i64) = self.conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(size_bytes),0), COALESCE(SUM(duration_ms),0)
                 FROM segments WHERE session_id=?1 AND deleted=0",
                [&session.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            if count == 0 {
                continue;
            }
            out.push(SessionSummary {
                session,
                segment_count: count,
                total_bytes: bytes,
                duration_ms: dur,
            });
        }
        Ok(out)
    }

    /// Resolve a segment id to its (path, deleted) — for `/seg/{id}.ts` serving.
    pub fn segment_by_id(&self, id: i64) -> Result<Option<(String, bool)>> {
        let row = self
            .conn
            .query_row(
                "SELECT path, deleted FROM segments WHERE id=?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)),
            )
            .optional()?;
        Ok(row)
    }

    /// (segment id, duration_ms) for a session's HLS playlist (ordered, deleted=0).
    pub fn segments_for_playlist(&self, session_id: &str) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, COALESCE(duration_ms,0) FROM segments
             WHERE session_id=?1 AND deleted=0 ORDER BY index_no",
        )?;
        let rows = stmt
            .query_map([session_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// (session_id, segment id, duration_ms) across all of a game's sessions in
    /// chronological order — for the whole-game playlist (insert DISCONTINUITY on
    /// session change).
    pub fn segments_for_game_playlist(&self, game_id: &str) -> Result<Vec<(String, i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.session_id, s.id, COALESCE(s.duration_ms,0)
             FROM segments s JOIN sessions ss ON s.session_id=ss.id
             WHERE ss.game_id=?1 AND s.deleted=0
             ORDER BY s.started_at, s.index_no",
        )?;
        let rows = stmt
            .query_map([game_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Ordered segment file paths for one session (for seamless playback).
    pub fn segment_paths_for_session(&self, session_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM segments WHERE session_id=?1 AND deleted=0 ORDER BY index_no",
        )?;
        let paths = stmt
            .query_map([session_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(paths)
    }

    /// All of a game's segment file paths in chronological order (all sessions).
    /// All sessions across all games (including archived ones), newest first.
    pub fn recent_sessions(&self, limit: i64) -> Result<Vec<RecentSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT ss.id,ss.game_id,ss.started_at,ss.ended_at,ss.codec_video,ss.codec_audio,ss.container,
                    ss.width,ss.height,ss.fps_num,ss.fps_den,ss.bitrate_video,ss.bitrate_audio,ss.storage_root,ss.status,
                    COALESCE(g.name,'?') AS game_name,
                    COUNT(s.id), COALESCE(SUM(s.size_bytes),0), COALESCE(SUM(s.duration_ms),0)
             FROM sessions ss
             LEFT JOIN games g ON g.id = ss.game_id
             JOIN segments s ON s.session_id = ss.id AND s.deleted = 0
             GROUP BY ss.id
             ORDER BY ss.started_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit], |r| {
                Ok(RecentSession {
                    game_name: r.get(15)?,
                    summary: SessionSummary {
                        session: Self::map_session(r)?,
                        segment_count: r.get(16)?,
                        total_bytes: r.get(17)?,
                        duration_ms: r.get(18)?,
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn segment_paths_for_game(&self, game_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.path FROM segments s JOIN sessions ss ON s.session_id=ss.id
             WHERE ss.game_id=?1 AND s.deleted=0
             ORDER BY s.started_at, s.index_no",
        )?;
        let paths = stmt
            .query_map([game_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(paths)
    }

    // ---- segments ----------------------------------------------------------

    /// Insert a freshly-opened segment (closed=0, deleted=0). Returns its id.
    pub fn insert_segment(
        &self,
        session_id: &str,
        path: &str,
        index_no: i64,
        started_at: i64,
        start_pts: Option<i64>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO segments
               (session_id,path,index_no,started_at,start_pts,size_bytes,protected,closed,deleted,deleting)
             VALUES (?1,?2,?3,?4,?5,0,0,0,0,0)",
            params![session_id, path, index_no, started_at, start_pts],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Finalise a segment (spec §16).
    pub fn close_segment(
        &self,
        id: i64,
        ended_at: i64,
        duration_ms: i64,
        size_bytes: i64,
        end_pts: Option<i64>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE segments
             SET ended_at=?2, duration_ms=?3, size_bytes=?4, end_pts=?5, closed=1
             WHERE id=?1",
            params![id, ended_at, duration_ms, size_bytes, end_pts],
        )?;
        Ok(())
    }

    pub fn mark_deleting(&self, id: i64) -> Result<()> {
        self.conn
            .execute("UPDATE segments SET deleting=1 WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn clear_deleting(&self, id: i64) -> Result<()> {
        self.conn
            .execute("UPDATE segments SET deleting=0 WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn mark_deleted(&self, id: i64) -> Result<()> {
        self.conn
            .execute("UPDATE segments SET deleted=1, deleting=0 WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn list_segments(&self, session_id: &str) -> Result<Vec<Segment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,session_id,path,index_no,started_at,ended_at,duration_ms,size_bytes,
                    start_pts,end_pts,protected,closed,deleted,deleting
             FROM segments WHERE session_id=?1 ORDER BY index_no",
        )?;
        let rows = stmt
            .query_map([session_id], Self::map_segment)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn map_segment(r: &rusqlite::Row) -> rusqlite::Result<Segment> {
        Ok(Segment {
            id: r.get(0)?,
            session_id: r.get(1)?,
            path: r.get(2)?,
            index_no: r.get(3)?,
            started_at: r.get(4)?,
            ended_at: r.get(5)?,
            duration_ms: r.get(6)?,
            size_bytes: r.get(7)?,
            start_pts: r.get(8)?,
            end_pts: r.get(9)?,
            protected: r.get::<_, i64>(10)? != 0,
            closed: r.get::<_, i64>(11)? != 0,
            deleted: r.get::<_, i64>(12)? != 0,
            deleting: r.get::<_, i64>(13)? != 0,
        })
    }

    // ---- retention support (spec §17) --------------------------------------

    /// Total bytes of non-deleted segments.
    pub fn total_size(&self) -> Result<i64> {
        let total =
            self.conn
                .query_row("SELECT COALESCE(SUM(size_bytes),0) FROM segments WHERE deleted=0", [], |r| {
                    r.get(0)
                })?;
        Ok(total)
    }

    /// Oldest segment eligible for deletion (spec §17): closed, not protected,
    /// not deleted/deleting, and not the active segment being written.
    pub fn oldest_deletable(&self, active_segment_id: Option<i64>) -> Result<Option<Segment>> {
        let active = active_segment_id.unwrap_or(-1);
        let seg = self
            .conn
            .query_row(
                "SELECT id,session_id,path,index_no,started_at,ended_at,duration_ms,size_bytes,
                        start_pts,end_pts,protected,closed,deleted,deleting
                 FROM segments
                 WHERE closed=1 AND protected=0 AND deleted=0 AND deleting=0 AND id<>?1
                 ORDER BY started_at ASC, index_no ASC
                 LIMIT 1",
                [active],
                Self::map_segment,
            )
            .optional()?;
        Ok(seg)
    }

    // ---- protection (spec §18) ---------------------------------------------

    /// Protect every closed segment of a session ending at or after `since_ms`.
    pub fn protect_recent(&self, session_id: &str, since_ms: i64) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE segments SET protected=1
             WHERE session_id=?1 AND ended_at>=?2 AND deleted=0",
            params![session_id, since_ms],
        )?;
        Ok(n)
    }

    pub fn protect_session(&self, session_id: &str) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE segments SET protected=1 WHERE session_id=?1 AND deleted=0",
            [session_id],
        )?;
        Ok(n)
    }

    pub fn set_protected(&self, segment_id: i64, protected: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE segments SET protected=?2 WHERE id=?1",
            params![segment_id, protected as i64],
        )?;
        Ok(())
    }

    // ---- integrity (spec §16) ----------------------------------------------

    /// Reconcile DB and filesystem. See [`IntegrityReport`].
    pub fn integrity_scan(&self, roots: &[PathBuf]) -> Result<IntegrityReport> {
        let mut report = IntegrityReport::default();

        // Pass 1: DB rows whose file vanished -> mark deleted; reconcile stale deleting.
        let mut known_paths: HashSet<PathBuf> = HashSet::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT id,path,deleting FROM segments WHERE deleted=0",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? != 0))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (id, path, deleting) in rows {
                let p = PathBuf::from(&path);
                let exists = p.exists();
                known_paths.insert(p);
                if !exists {
                    self.mark_deleted(id)?;
                    report.missing_marked_deleted.push(id);
                } else if deleting {
                    // crashed mid-delete but file survived: clear the flag.
                    self.clear_deleting(id)?;
                    report.stale_deleting_cleared.push(id);
                }
            }
        }

        // Pass 2: walk roots for orphan `.ts` and incomplete `.ts.writing`.
        for root in roots {
            walk_ts(root, &mut |p| {
                let name = p.to_string_lossy();
                if name.ends_with(".ts.writing") {
                    report.writing_incomplete.push(p.clone());
                } else if name.ends_with(".ts") && !known_paths.contains(p) {
                    report.orphans.push(p.clone());
                }
            });
        }

        Ok(report)
    }
}

/// Result of an [`Store::integrity_scan`].
#[derive(Debug, Default)]
pub struct IntegrityReport {
    /// Segment ids whose file was gone (now `deleted=1`).
    pub missing_marked_deleted: Vec<i64>,
    /// Segment ids whose stale `deleting` flag was cleared.
    pub stale_deleting_cleared: Vec<i64>,
    /// `*.ts.writing` files left by a crash (incomplete; need recovery).
    pub writing_incomplete: Vec<PathBuf>,
    /// `*.ts` files on disk with no DB row.
    pub orphans: Vec<PathBuf>,
}

impl IntegrityReport {
    pub fn is_clean(&self) -> bool {
        self.missing_marked_deleted.is_empty()
            && self.stale_deleting_cleared.is_empty()
            && self.writing_incomplete.is_empty()
            && self.orphans.is_empty()
    }
}

/// Recursively visit files under `dir`, calling `f` for each regular file.
fn walk_ts(dir: &Path, f: &mut impl FnMut(&PathBuf)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_ts(&path, f);
        } else {
            f(&path);
        }
    }
}

/// SQLite schema, verbatim from spec §6/§7/§16 (with the §16 `deleting` column).
const SCHEMA_V1: &str = r#"
CREATE TABLE games (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    launch_command TEXT NOT NULL,
    launch_workdir TEXT,
    launch_args TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE game_window_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id TEXT NOT NULL,
    exe_path TEXT,
    process_name TEXT,
    window_title_pattern TEXT,
    window_class TEXT,
    app_user_model_id TEXT,
    preferred_monitor_index INTEGER,
    last_hwnd INTEGER,
    confidence INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(game_id) REFERENCES games(id)
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    game_id TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    codec_video TEXT NOT NULL,
    codec_audio TEXT NOT NULL,
    container TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    fps_num INTEGER,
    fps_den INTEGER,
    bitrate_video INTEGER,
    bitrate_audio INTEGER,
    storage_root TEXT NOT NULL,
    status TEXT NOT NULL,
    FOREIGN KEY(game_id) REFERENCES games(id)
);

CREATE TABLE segments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    path TEXT NOT NULL,
    index_no INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    duration_ms INTEGER,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    start_pts INTEGER,
    end_pts INTEGER,
    protected INTEGER NOT NULL DEFAULT 0,
    closed INTEGER NOT NULL DEFAULT 0,
    deleted INTEGER NOT NULL DEFAULT 0,
    deleting INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(session_id) REFERENCES sessions(id)
);

CREATE INDEX idx_segments_gc
ON segments(deleted, protected, closed, started_at);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_game() -> Game {
        Game {
            id: "g1".into(),
            name: "Forza Horizon 5".into(),
            launch_command: "forza.exe".into(),
            launch_workdir: None,
            launch_args: None,
            auto_record: false,
            preset: None,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    fn sample_session() -> Session {
        Session {
            id: "s1".into(),
            game_id: "g1".into(),
            started_at: 2000,
            ended_at: None,
            codec_video: "hevc".into(),
            codec_audio: "aac".into(),
            container: "mpegts".into(),
            width: Some(2560),
            height: Some(1440),
            fps_num: Some(60),
            fps_den: Some(1),
            bitrate_video: Some(35_000_000),
            bitrate_audio: Some(192_000),
            storage_root: "D:/GameRecordings".into(),
            status: SessionStatus::Recording,
        }
    }

    #[test]
    fn migrate_and_game_crud() {
        let s = Store::open_in_memory().unwrap();
        s.upsert_game(&sample_game()).unwrap();
        let g = s.get_game("g1").unwrap().unwrap();
        assert_eq!(g.name, "Forza Horizon 5");
        assert_eq!(s.list_games().unwrap().len(), 1);
    }

    #[test]
    fn window_rule_replace() {
        let s = Store::open_in_memory().unwrap();
        s.upsert_game(&sample_game()).unwrap();
        let mut rule = WindowRule {
            game_id: "g1".into(),
            process_name: Some("forza.exe".into()),
            confidence: 1,
            ..Default::default()
        };
        s.upsert_window_rule(&rule).unwrap();
        rule.process_name = Some("forza2.exe".into());
        s.upsert_window_rule(&rule).unwrap();
        let got = s.get_window_rule("g1").unwrap().unwrap();
        assert_eq!(got.process_name.as_deref(), Some("forza2.exe"));
    }

    #[test]
    fn segment_lifecycle_and_total() {
        let s = Store::open_in_memory().unwrap();
        s.upsert_game(&sample_game()).unwrap();
        s.insert_session(&sample_session()).unwrap();

        let id = s.insert_segment("s1", "D:/r/seg_000001.ts", 1, 2000, Some(0)).unwrap();
        // open segment isn't deletable yet (closed=0)
        assert!(s.oldest_deletable(None).unwrap().is_none());
        s.close_segment(id, 2600, 600_000, 1_000_000, Some(54_000_000)).unwrap();
        assert_eq!(s.total_size().unwrap(), 1_000_000);

        let oldest = s.oldest_deletable(None).unwrap().unwrap();
        assert_eq!(oldest.id, id);
        // protect it -> no longer deletable
        s.set_protected(id, true).unwrap();
        assert!(s.oldest_deletable(None).unwrap().is_none());
    }

    #[test]
    fn oldest_skips_active() {
        let s = Store::open_in_memory().unwrap();
        s.upsert_game(&sample_game()).unwrap();
        s.insert_session(&sample_session()).unwrap();
        let a = s.insert_segment("s1", "a.ts", 1, 2000, None).unwrap();
        let b = s.insert_segment("s1", "b.ts", 2, 2100, None).unwrap();
        s.close_segment(a, 1, 1, 10, None).unwrap();
        s.close_segment(b, 1, 1, 10, None).unwrap();
        // marking `a` active means `b` is the oldest deletable
        assert_eq!(s.oldest_deletable(Some(a)).unwrap().unwrap().id, b);
    }

    #[test]
    fn protect_recent_window() {
        let s = Store::open_in_memory().unwrap();
        s.upsert_game(&sample_game()).unwrap();
        s.insert_session(&sample_session()).unwrap();
        let old = s.insert_segment("s1", "old.ts", 1, 1000, None).unwrap();
        let new = s.insert_segment("s1", "new.ts", 2, 2000, None).unwrap();
        s.close_segment(old, 1_000, 1, 1, None).unwrap();
        s.close_segment(new, 5_000, 1, 1, None).unwrap();
        // protect segments ending at/after 3000 -> only `new`
        assert_eq!(s.protect_recent("s1", 3_000).unwrap(), 1);
        let segs = s.list_segments("s1").unwrap();
        assert!(!segs.iter().find(|x| x.id == old).unwrap().protected);
        assert!(segs.iter().find(|x| x.id == new).unwrap().protected);
    }

    #[test]
    fn integrity_marks_missing() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open_in_memory().unwrap();
        s.upsert_game(&sample_game()).unwrap();
        s.insert_session(&sample_session()).unwrap();
        let missing = dir.path().join("gone.ts");
        let id = s.insert_segment("s1", missing.to_str().unwrap(), 1, 1, None).unwrap();
        s.close_segment(id, 1, 1, 100, None).unwrap();

        let report = s.integrity_scan(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(report.missing_marked_deleted, vec![id]);
        // now deleted -> excluded from totals
        assert_eq!(s.total_size().unwrap(), 0);
    }

    #[test]
    fn integrity_finds_writing_and_orphan() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("seg_000001.ts.writing"), b"partial").unwrap();
        std::fs::write(dir.path().join("seg_000002.ts"), b"orphan").unwrap();
        let s = Store::open_in_memory().unwrap();
        let report = s.integrity_scan(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(report.writing_incomplete.len(), 1);
        assert_eq!(report.orphans.len(), 1);
        assert!(!report.is_clean());
    }
}
