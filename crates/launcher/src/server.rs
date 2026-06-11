//! Embedded HTTP server: browser-based recordings viewer + HLS (spec extension).
//!
//! Our recording segments are ~1GB each — far too large to serve as HLS segments
//! directly. So per session we run ffmpeg `-c copy` (no re-encode, just
//! re-packages the existing `.ts`) into short 6s HLS segments cached under
//! `<storage_root>/_hlscache/<sid>/`, and serve that. A `_hls264/` variant
//! re-encodes to H.264 for browsers/devices that can't decode HEVC.
//!
//! Binds 0.0.0.0 (LAN) so phones/other PCs on the network can watch.

use rec_core::store::Store;
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tiny_http::{Header, Response, ResponseBox, Server, StatusCode};

const PREFERRED_PORT: u16 = 8787;

struct Ctx {
    db_path: PathBuf,
    storage_root: PathBuf,
    screenshot_dir: PathBuf,
    ffmpeg: String,
    /// HLS re-segmentation jobs already started (key: `sid` or `264:sid`).
    started: Mutex<HashSet<String>>,
    /// clip jobs: job_id -> state. Arc so the worker thread can update it.
    clips: Arc<Mutex<std::collections::HashMap<String, ClipState>>>,
    clip_seq: std::sync::atomic::AtomicU64,
}

#[derive(Clone)]
enum ClipState {
    /// In progress, with a 0.0..=1.0 fraction (from ffmpeg `-progress`).
    Running(f32),
    Done(PathBuf),
    Failed(String),
}

/// A running viewer server.
pub struct ViewerServer {
    pub addr: std::net::SocketAddr,
    server: Arc<Server>,
    stop: Arc<AtomicBool>,
    _workers: Vec<JoinHandle<()>>,
}

impl ViewerServer {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.addr.port())
    }
    /// LAN URL using the machine's primary IPv4 (best-effort), for phones etc.
    pub fn lan_url(&self) -> Option<String> {
        local_ipv4().map(|ip| format!("http://{}:{}/", ip, self.addr.port()))
    }
}

impl Drop for ViewerServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.server.unblock();
    }
}

/// Start the server on a background thread pool. Tries port 8787, else a random
/// free port.
pub fn start(
    db_path: PathBuf,
    storage_root: PathBuf,
    screenshot_dir: PathBuf,
    ffmpeg: String,
) -> anyhow::Result<ViewerServer> {
    // Pick a port: prefer 8787, fall back to OS-assigned.
    let port = if TcpListener::bind(("0.0.0.0", PREFERRED_PORT)).is_ok() {
        PREFERRED_PORT
    } else {
        0
    };
    let server = Server::http(("0.0.0.0", port))
        .map_err(|e| anyhow::anyhow!("HTTP server bind failed: {e}"))?;
    let addr = server.server_addr().to_ip().ok_or_else(|| anyhow::anyhow!("no ip addr"))?;
    let server = Arc::new(server);

    let ctx = Arc::new(Ctx {
        db_path,
        storage_root,
        screenshot_dir,
        ffmpeg,
        started: Mutex::new(HashSet::new()),
        clips: Arc::new(Mutex::new(std::collections::HashMap::new())),
        clip_seq: std::sync::atomic::AtomicU64::new(1),
    });
    let stop = Arc::new(AtomicBool::new(false));

    let mut workers = Vec::new();
    for _ in 0..4 {
        let server = server.clone();
        let ctx = ctx.clone();
        let stop = stop.clone();
        workers.push(std::thread::spawn(move || {
            while let Ok(mut req) = server.recv() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let resp = handle(&mut req, &ctx);
                let _ = req.respond(resp);
            }
        }));
    }

    tracing::info!(%addr, "viewer server started");
    Ok(ViewerServer { addr, server, stop, _workers: workers })
}

// ---------------------------------------------------------------------------

fn handle(req: &mut tiny_http::Request, ctx: &Ctx) -> ResponseBox {
    let raw = req.url().to_string();
    let method = req.method().clone();
    let path = raw.split('?').next().unwrap_or("/");
    let segs: Vec<String> = path.trim_matches('/').split('/').map(|s| s.to_string()).collect();
    let segs: Vec<&str> = segs.iter().map(|s| s.as_str()).collect();

    match (&method, segs.as_slice()) {
        (tiny_http::Method::Get, [""]) | (tiny_http::Method::Get, []) => {
            asset("index.html")
        }
        (tiny_http::Method::Get, ["static", file]) => asset(file),

        (tiny_http::Method::Get, ["api", "games"]) => api_games(ctx),
        (tiny_http::Method::Get, ["api", "games", gid, "sessions"]) => api_sessions(ctx, gid),
        (tiny_http::Method::Get, ["api", "sessions"]) => api_recent(ctx),

        (tiny_http::Method::Get, ["hls", "session", m3u8]) => {
            hls_playlist(ctx, strip_ext(m3u8, ".m3u8"), false, false)
        }
        (tiny_http::Method::Get, ["hls264", "session", m3u8]) => {
            hls_playlist(ctx, strip_ext(m3u8, ".m3u8"), false, true)
        }
        (tiny_http::Method::Get, ["hls", "game", m3u8]) => {
            hls_playlist(ctx, strip_ext(m3u8, ".m3u8"), true, false)
        }
        (tiny_http::Method::Get, ["hlscache", id, file]) => {
            serve_cache_file(ctx, "_hlscache", id, file)
        }
        (tiny_http::Method::Get, ["hlscache264", id, file]) => {
            serve_cache_file(ctx, "_hls264", id, file)
        }

        (tiny_http::Method::Post, ["api", "clip"]) => api_clip(req, ctx),
        (tiny_http::Method::Get, ["api", "clip", job]) => api_clip_status(ctx, job),
        (tiny_http::Method::Get, ["clips", file]) => serve_clip(ctx, file),
        (tiny_http::Method::Get, ["api", "frame"]) => api_frame(ctx, &raw),
        (tiny_http::Method::Post, ["api", "screenshot"]) => api_screenshot(req, ctx),

        _ => text(404, "not found"),
    }
}

// ---- static assets --------------------------------------------------------

fn asset(name: &str) -> ResponseBox {
    let (bytes, ctype): (&[u8], &str) = match name {
        "index.html" => (include_bytes!("../assets/web/index.html"), "text/html; charset=utf-8"),
        "app.js" => (include_bytes!("../assets/web/app.js"), "application/javascript; charset=utf-8"),
        "app.css" => (include_bytes!("../assets/web/app.css"), "text/css; charset=utf-8"),
        "hls.min.js" => (include_bytes!("../assets/web/hls.min.js"), "application/javascript; charset=utf-8"),
        _ => return text(404, "not found"),
    };
    Response::from_data(bytes.to_vec())
        .with_header(ctype_header(ctype))
        .boxed()
}

// ---- JSON API -------------------------------------------------------------

fn api_games(ctx: &Ctx) -> ResponseBox {
    let Ok(store) = Store::open(&ctx.db_path) else {
        return text(500, "db error");
    };
    let games = store.list_games().unwrap_or_default();
    let json: Vec<_> = games
        .iter()
        .map(|g| serde_json::json!({"id": g.id, "name": g.name}))
        .collect();
    json_response(&serde_json::Value::Array(json))
}

fn api_sessions(ctx: &Ctx, gid: &str) -> ResponseBox {
    let Ok(store) = Store::open(&ctx.db_path) else {
        return text(500, "db error");
    };
    let sums = store.session_summaries(gid).unwrap_or_default();
    let json: Vec<_> = sums
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.session.id,
                "started_at": s.session.started_at,
                "duration_ms": s.duration_ms,
                "total_bytes": s.total_bytes,
                "segment_count": s.segment_count,
            })
        })
        .collect();
    json_response(&serde_json::Value::Array(json))
}

/// Recent sessions across all games (works for deleted/archived games too).
fn api_recent(ctx: &Ctx) -> ResponseBox {
    let Ok(store) = Store::open(&ctx.db_path) else {
        return text(500, "db error");
    };
    let recent = store.recent_sessions(100).unwrap_or_default();
    let json: Vec<_> = recent
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.summary.session.id,
                "game_id": r.summary.session.game_id,
                "game_name": r.game_name,
                "started_at": r.summary.session.started_at,
                "duration_ms": r.summary.duration_ms,
                "total_bytes": r.summary.total_bytes,
                "segment_count": r.summary.segment_count,
            })
        })
        .collect();
    json_response(&serde_json::Value::Array(json))
}

// ---- HLS ------------------------------------------------------------------

/// Ensure an HLS re-segmentation job exists; return the index.m3u8 path if ready.
fn ensure_hls(ctx: &Ctx, id: &str, whole_game: bool, h264: bool) -> Option<PathBuf> {
    let sub = if h264 { "_hls264" } else { "_hlscache" };
    let cache = ctx.storage_root.join(sub).join(id);
    let index = cache.join("index.m3u8");
    if index.exists() {
        return Some(index);
    }
    let key = format!("{}{}", if h264 { "264:" } else { "" }, id);
    let mut started = ctx.started.lock().unwrap();
    if started.contains(&key) {
        return index.exists().then_some(index);
    }

    // Gather source segment paths.
    let store = Store::open(&ctx.db_path).ok()?;
    let paths: Vec<String> = if whole_game {
        store.segment_paths_for_game(id).ok()?
    } else {
        store.segment_paths_for_session(id).ok()?
    }
    .into_iter()
    .filter(|p| Path::new(p).exists())
    .collect();
    if paths.is_empty() {
        return None;
    }

    std::fs::create_dir_all(&cache).ok()?;
    let base = format!("/{}/{}/", if h264 { "hlscache264" } else { "hlscache" }, id);
    let input = format!("concat:{}", paths.join("|"));
    let mut cmd = Command::new(&ctx.ffmpeg);
    cmd.args(["-y", "-loglevel", "error", "-i", &input]);
    if h264 {
        cmd.args(["-c:v", "h264_nvenc", "-preset", "p5", "-b:v", "12M", "-c:a", "aac"]);
    } else {
        cmd.args(["-c", "copy"]);
    }
    cmd.args([
        "-f", "hls",
        "-hls_time", "6",
        "-hls_playlist_type", "event",
        "-hls_flags", "independent_segments",
        "-hls_base_url", &base,
        "-hls_segment_filename", cache.join("s%05d.ts").to_str()?,
        cache.join("index.m3u8").to_str()?,
    ]);
    match cmd.spawn() {
        Ok(_) => {
            tracing::info!(id, h264, "started HLS re-segmentation");
            started.insert(key);
            None // not ready yet; client polls
        }
        Err(e) => {
            tracing::warn!(error=%e, "failed to start ffmpeg HLS job");
            None
        }
    }
}

fn hls_playlist(ctx: &Ctx, id: &str, whole_game: bool, h264: bool) -> ResponseBox {
    match ensure_hls(ctx, id, whole_game, h264) {
        Some(index) => match std::fs::read(&index) {
            Ok(bytes) => Response::from_data(bytes)
                .with_header(ctype_header("application/vnd.apple.mpegurl"))
                .with_header(no_cache())
                .boxed(),
            Err(_) => text(500, "playlist read error"),
        },
        // Not ready yet — client polls until 200.
        None => text(425, "generating"),
    }
}

fn serve_cache_file(ctx: &Ctx, sub: &str, id: &str, file: &str) -> ResponseBox {
    if !safe_name(id) || !safe_name(file) {
        return text(400, "bad name");
    }
    let p = ctx.storage_root.join(sub).join(id).join(file);
    let ctype = if file.ends_with(".m3u8") {
        "application/vnd.apple.mpegurl"
    } else {
        "video/mp2t"
    };
    serve_file(&p, ctype)
}

// ---- clip + screenshot ----------------------------------------------------

fn api_clip(req: &mut tiny_http::Request, ctx: &Ctx) -> ResponseBox {
    // body: {session_id, start_ms, end_ms, mode, title}
    let mut body = String::new();
    if req.as_reader().read_to_string(&mut body).is_err() {
        return text(400, "bad body");
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
        return text(400, "bad json");
    };
    let sid = v["session_id"].as_str().unwrap_or("");
    let start = v["start_ms"].as_i64().unwrap_or(0).max(0);
    let end = v["end_ms"].as_i64().unwrap_or(0);
    let reencode = v["mode"].as_str() != Some("copy");
    let title = v["title"].as_str().unwrap_or("clip");
    if sid.is_empty() || end <= start {
        return text(400, "bad params");
    }

    let Ok(store) = Store::open(&ctx.db_path) else {
        return text(500, "db");
    };
    let paths: Vec<String> = store
        .segment_paths_for_session(sid)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| Path::new(p).exists())
        .collect();
    if paths.is_empty() {
        return text(404, "no segments");
    }

    let job = format!("clip{}", ctx.clip_seq.fetch_add(1, Ordering::Relaxed));
    let clips_dir = ctx.storage_root.join("_clips");
    let _ = std::fs::create_dir_all(&clips_dir);

    // Output filename: "<title>_<start-clock>.mp4" (e.g. Shadowverse_00-12-34.mp4),
    // de-duplicated with a numeric suffix if it already exists.
    let base = format!("{}_{}", sanitize_label(title), clock_label(start));
    let mut out = clips_dir.join(format!("{base}.mp4"));
    let mut n = 2;
    while out.exists() {
        out = clips_dir.join(format!("{base}_{n}.mp4"));
        n += 1;
    }
    ctx.clips.lock().unwrap().insert(job.clone(), ClipState::Running(0.0));

    let input = format!("concat:{}", paths.join("|"));
    let start_s = format!("{:.3}", start as f64 / 1000.0);
    let total_ms = (end - start).max(1);
    let dur_s = format!("{:.3}", total_ms as f64 / 1000.0);
    let ffmpeg = ctx.ffmpeg.clone();
    let clips = ctx.clips.clone();
    let out2 = out.clone();
    let job2 = job.clone();
    std::thread::spawn(move || {
        let mut cmd = Command::new(&ffmpeg);
        cmd.args([
            "-y", "-nostats", "-loglevel", "error", "-progress", "pipe:1",
            "-ss", &start_s, "-i", &input, "-t", &dur_s,
        ]);
        if reencode {
            cmd.args(["-c:v", "hevc_nvenc", "-preset", "p5", "-b:v", "30M", "-c:a", "aac"]);
        } else {
            cmd.args(["-c", "copy"]);
        }
        cmd.args(["-movflags", "+faststart", out2.to_str().unwrap_or("clip.mp4")]);
        cmd.stdout(Stdio::piped()).stderr(Stdio::null());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                clips.lock().unwrap().insert(job2, ClipState::Failed(e.to_string()));
                return;
            }
        };
        // Parse ffmpeg's `-progress` stream for `out_time_us` → fraction done.
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<i64>().ok()) {
                    let frac = ((us as f64 / 1000.0) / total_ms as f64).clamp(0.0, 1.0) as f32;
                    clips.lock().unwrap().insert(job2.clone(), ClipState::Running(frac));
                }
            }
        }
        let state = match child.wait() {
            Ok(s) if s.success() => ClipState::Done(out2),
            Ok(s) => ClipState::Failed(format!("ffmpeg exit {s}")),
            Err(e) => ClipState::Failed(e.to_string()),
        };
        clips.lock().unwrap().insert(job2, state);
    });

    json_response(&serde_json::json!({"job": job}))
}

/// Receive a PNG screenshot from the web viewer and save it under the
/// app-configured screenshot directory. Body is the raw PNG; `?name=` gives a
/// label (game name / timestamp) used in the filename.
fn api_screenshot(req: &mut tiny_http::Request, ctx: &Ctx) -> ResponseBox {
    let raw = req.url().to_string();
    let mut label = String::new();
    if let Some(q) = raw.split('?').nth(1) {
        for kv in q.split('&') {
            if let Some(v) = kv.strip_prefix("name=") {
                label = url_decode(v);
            }
        }
    }
    let mut bytes = Vec::new();
    if req.as_reader().read_to_end(&mut bytes).is_err() || bytes.is_empty() {
        return text(400, "bad body");
    }
    // Basic PNG signature check.
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return text(400, "not a png");
    }
    if let Err(e) = std::fs::create_dir_all(&ctx.screenshot_dir) {
        return text(500, &format!("mkdir: {e}"));
    }
    let slug = sanitize_label(&label);
    let stamp = file_stamp(&bytes);
    let name = format!("{slug}_{stamp}.png");
    let path = ctx.screenshot_dir.join(&name);
    match std::fs::write(&path, &bytes) {
        Ok(()) => {
            tracing::info!(path = %path.display(), "saved screenshot");
            json_response(&serde_json::json!({
                "ok": true,
                "path": path.to_string_lossy(),
                "name": name,
            }))
        }
        Err(e) => text(500, &format!("write: {e}")),
    }
}

fn api_clip_status(ctx: &Ctx, job: &str) -> ResponseBox {
    let state = ctx.clips.lock().unwrap().get(job).cloned();
    match state {
        Some(ClipState::Running(p)) => {
            json_response(&serde_json::json!({"status":"running","progress": p}))
        }
        Some(ClipState::Done(p)) => {
            let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            // Serve by job id (ascii-safe); the unicode filename is conveyed in
            // `name` for the browser's download attribute.
            json_response(&serde_json::json!({
                "status":"done", "url": format!("/clips/{job}"), "name": name,
            }))
        }
        Some(ClipState::Failed(e)) => json_response(&serde_json::json!({"status":"failed","error":e})),
        None => text(404, "no such job"),
    }
}

/// Serve a finished clip by its job id (looked up in the clips map, so no path
/// is taken from the URL and unicode filenames stay safe).
fn serve_clip(ctx: &Ctx, job: &str) -> ResponseBox {
    let path = match ctx.clips.lock().unwrap().get(job) {
        Some(ClipState::Done(p)) => p.clone(),
        _ => return text(404, "not ready"),
    };
    serve_file(&path, "video/mp4")
}

fn api_frame(ctx: &Ctx, raw_url: &str) -> ResponseBox {
    // /api/frame?session_id=..&t_ms=..
    let q = raw_url.split('?').nth(1).unwrap_or("");
    let mut sid = String::new();
    let mut t_ms = 0i64;
    for kv in q.split('&') {
        let mut it = kv.splitn(2, '=');
        match (it.next(), it.next()) {
            (Some("session_id"), Some(v)) => sid = v.to_string(),
            (Some("t_ms"), Some(v)) => t_ms = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    let Ok(store) = Store::open(&ctx.db_path) else { return text(500, "db") };
    let paths: Vec<String> = store
        .segment_paths_for_session(&sid)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| Path::new(p).exists())
        .collect();
    if paths.is_empty() {
        return text(404, "no segments");
    }
    let input = format!("concat:{}", paths.join("|"));
    let out = std::env::temp_dir().join(format!("frame_{}.png", t_ms));
    let ok = Command::new(&ctx.ffmpeg)
        .args(["-y", "-loglevel", "error", "-ss", &format!("{:.3}", t_ms as f64 / 1000.0), "-i", &input, "-frames:v", "1"])
        .arg(&out)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        serve_file(&out, "image/png")
    } else {
        text(500, "frame extract failed")
    }
}

// ---- helpers --------------------------------------------------------------

fn serve_file(path: &Path, ctype: &str) -> ResponseBox {
    match std::fs::File::open(path) {
        Ok(file) => {
            let len = file.metadata().map(|m| m.len() as usize).ok();
            Response::new(StatusCode(200), vec![ctype_header(ctype)], file, len, None).boxed()
        }
        Err(_) => text(404, "not found"),
    }
}

fn text(code: u16, msg: &str) -> ResponseBox {
    Response::from_string(msg).with_status_code(StatusCode(code)).boxed()
}

fn json_response(v: &serde_json::Value) -> ResponseBox {
    Response::from_string(v.to_string())
        .with_header(ctype_header("application/json; charset=utf-8"))
        .boxed()
}

fn ctype_header(v: &str) -> Header {
    Header::from_bytes(&b"Content-Type"[..], v.as_bytes()).unwrap()
}
fn no_cache() -> Header {
    Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap()
}

fn strip_ext<'a>(s: &'a str, ext: &str) -> &'a str {
    s.strip_suffix(ext).unwrap_or(s)
}

/// Allow only `[A-Za-z0-9._-]` (no path traversal).
fn safe_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
        && !s.contains("..")
}

/// `HH-MM-SS` from a millisecond offset, for clip filenames.
fn clock_label(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    format!("{:02}-{:02}-{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Turn a free-form label into a safe filename stem (keeps unicode letters).
fn sanitize_label(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "shot".into()
    } else {
        trimmed.chars().take(60).collect()
    }
}

/// `YYYYMMDD_HHMMSS_xxxx` in local time, with a short content hash so two shots
/// in the same second don't collide.
fn file_stamp(bytes: &[u8]) -> String {
    use time::OffsetDateTime;
    let now = OffsetDateTime::now_utc();
    let now = time::UtcOffset::current_local_offset()
        .map(|o| now.to_offset(o))
        .unwrap_or(now);
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}_{:04x}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        short_hash(bytes),
    )
}

fn short_hash(bytes: &[u8]) -> u16 {
    let mut h: u32 = 2_166_136_261;
    for (i, b) in bytes.iter().enumerate().step_by(257) {
        h = (h ^ *b as u32).wrapping_mul(16_777_619).wrapping_add(i as u32);
    }
    (h & 0xffff) as u16
}

/// Minimal percent-decoding for query values (`%20`, `+`).
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => match (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn local_ipv4() -> Option<String> {
    // Connect a UDP socket to a public IP to discover the outbound interface IP.
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    sock.local_addr().ok().map(|a| a.ip().to_string())
}
