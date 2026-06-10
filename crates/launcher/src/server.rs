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
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tiny_http::{Header, Response, ResponseBox, Server, StatusCode};

const PREFERRED_PORT: u16 = 8787;

struct Ctx {
    db_path: PathBuf,
    storage_root: PathBuf,
    ffmpeg: String,
    /// HLS re-segmentation jobs already started (key: `sid` or `264:sid`).
    started: Mutex<HashSet<String>>,
    /// clip jobs: job_id -> state. Arc so the worker thread can update it.
    clips: Arc<Mutex<std::collections::HashMap<String, ClipState>>>,
    clip_seq: std::sync::atomic::AtomicU64,
}

#[derive(Clone)]
enum ClipState {
    Running,
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
pub fn start(db_path: PathBuf, storage_root: PathBuf, ffmpeg: String) -> anyhow::Result<ViewerServer> {
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
    // body: {session_id, start_ms, end_ms, mode}
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
    let out = clips_dir.join(format!("{job}.mp4"));
    ctx.clips.lock().unwrap().insert(job.clone(), ClipState::Running);

    let input = format!("concat:{}", paths.join("|"));
    let start_s = format!("{:.3}", start as f64 / 1000.0);
    let dur_s = format!("{:.3}", (end - start) as f64 / 1000.0);
    let ffmpeg = ctx.ffmpeg.clone();
    let clips = ctx.clips.clone();
    let out2 = out.clone();
    let job2 = job.clone();
    std::thread::spawn(move || {
        let mut cmd = Command::new(&ffmpeg);
        cmd.args(["-y", "-loglevel", "error", "-ss", &start_s, "-i", &input, "-t", &dur_s]);
        if reencode {
            cmd.args(["-c:v", "hevc_nvenc", "-preset", "p5", "-b:v", "30M", "-c:a", "aac"]);
        } else {
            cmd.args(["-c", "copy"]);
        }
        cmd.args(["-movflags", "+faststart", out2.to_str().unwrap_or("clip.mp4")]);
        let state = match cmd.status() {
            Ok(s) if s.success() => ClipState::Done(out2),
            Ok(s) => ClipState::Failed(format!("ffmpeg exit {s}")),
            Err(e) => ClipState::Failed(e.to_string()),
        };
        clips.lock().unwrap().insert(job2, state);
    });

    json_response(&serde_json::json!({"job": job}))
}

fn api_clip_status(ctx: &Ctx, job: &str) -> ResponseBox {
    let state = ctx.clips.lock().unwrap().get(job).cloned();
    match state {
        Some(ClipState::Running) => json_response(&serde_json::json!({"status":"running"})),
        Some(ClipState::Done(p)) => {
            let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            json_response(&serde_json::json!({"status":"done","url":format!("/clips/{name}")}))
        }
        Some(ClipState::Failed(e)) => json_response(&serde_json::json!({"status":"failed","error":e})),
        None => text(404, "no such job"),
    }
}

fn serve_clip(ctx: &Ctx, file: &str) -> ResponseBox {
    if !safe_name(file) {
        return text(400, "bad name");
    }
    serve_file(&ctx.storage_root.join("_clips").join(file), "video/mp4")
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

fn local_ipv4() -> Option<String> {
    // Connect a UDP socket to a public IP to discover the outbound interface IP.
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    sock.local_addr().ok().map(|a| a.ip().to_string())
}
