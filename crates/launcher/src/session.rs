//! Recording session orchestration (spec §20). Windows-only.
//!
//! Launches the game, finds its window, then runs the recorder pipeline in-process
//! on a worker thread until the user stops or the game exits.

use crate::matcher;
use crate::process_util;
use crate::window_detect;
use crossbeam_channel::{unbounded, Receiver, Sender};
use rec_core::domain::{Game, Session, SessionStatus, WindowRule};
use rec_core::fsutil::{now_ms, slugify};
use rec_core::preset::RecordingPreset;
use rec_core::protocol::RecorderMsg;
use rec_core::store::Store;
use recorder::pipeline::{record, RecordParams};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// A running recording the UI can observe and stop.
pub struct Recording {
    pub game_name: String,
    pub stop: Arc<AtomicBool>,
    pub rx: Receiver<RecorderMsg>,
    pub handle: Option<JoinHandle<()>>,
}

impl Recording {
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(|h| h.is_finished())
    }
}

/// Start launching + recording `game`. Returns immediately; work runs on a thread.
/// Launch the game, then record it.
pub fn start(game: Game, preset: RecordingPreset, storage_root: PathBuf, db_path: PathBuf) -> Recording {
    spawn(game, preset, storage_root, db_path, true)
}

/// Record a game that is ALREADY running (auto-record): attach without launching.
pub fn start_attach(game: Game, preset: RecordingPreset, storage_root: PathBuf, db_path: PathBuf) -> Recording {
    spawn(game, preset, storage_root, db_path, false)
}

fn spawn(
    game: Game,
    preset: RecordingPreset,
    storage_root: PathBuf,
    db_path: PathBuf,
    do_launch: bool,
) -> Recording {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = unbounded();
    let game_name = game.name.clone();
    let stop_thread = stop.clone();
    let tx_thread = tx.clone();

    let handle = std::thread::spawn(move || {
        if let Err(e) = run(game, preset, storage_root, db_path, do_launch, stop_thread, tx_thread.clone()) {
            let _ = tx_thread.send(RecorderMsg::Error {
                code: "SESSION".into(),
                message: e.to_string(),
            });
        }
    });

    Recording {
        game_name,
        stop,
        rx,
        handle: Some(handle),
    }
}

fn run(
    game: Game,
    preset: RecordingPreset,
    storage_root: PathBuf,
    db_path: PathBuf,
    do_launch: bool,
    stop: Arc<AtomicBool>,
    tx: Sender<RecorderMsg>,
) -> anyhow::Result<()> {
    let store = Store::open(&db_path)?;
    store.upsert_game(&game)?; // ensure FK targets exist before rules/sessions
    let rule = store.get_window_rule(&game.id)?;

    // Launch the game (unless we're attaching to an already-running one).
    let launched_pid = if do_launch {
        let launched = process_util::launch(
            &game.launch_command,
            game.launch_args.as_deref(),
            game.launch_workdir.as_deref(),
        )?;
        launched.process_id
    } else {
        None
    };
    let _ = tx.send(RecorderMsg::Status {
        state: "waiting_for_window".into(),
        duration_ms: None,
        size_bytes: None,
    });

    // Find the target window. Bootstrapper games (e.g. Shadowverse) exit the
    // launched process and reopen the real game under a new PID, so we also match
    // by the registered exe name and require the window to be *stable*.
    let exe = target_exe_name(&game.launch_command);
    tracing::info!(?launched_pid, ?exe, do_launch, name = %game.name, "searching for game window");
    let target = wait_for_target(
        launched_pid,
        exe.as_deref(),
        rule.as_ref(),
        &stop,
        Duration::from_secs(40),
    );
    let (hwnd, pid, proc_name) = match target {
        Some(t) => t,
        None => anyhow::bail!("target window not found"),
    };
    tracing::info!(hwnd, pid, proc = ?proc_name, "committed target window; attaching capture");

    // Persist/refresh the window rule for next time.
    store.upsert_window_rule(&WindowRule {
        game_id: game.id.clone(),
        process_name: proc_name,
        last_hwnd: Some(hwnd),
        confidence: 1,
        ..Default::default()
    })?;

    // Create the session.
    let slug = slugify(&game.name);
    let session_start = timestamp();
    let session_id = uuid::Uuid::new_v4().to_string();
    let output_dir = storage_root.join(&slug).join(&session_start);
    std::fs::create_dir_all(&output_dir)?;

    store.insert_session(&Session {
        id: session_id.clone(),
        game_id: game.id.clone(),
        started_at: now_ms(),
        ended_at: None,
        codec_video: "hevc".into(),
        codec_audio: "aac".into(),
        container: "mpegts".into(),
        width: Some(preset.video.width),
        height: Some(preset.video.height),
        fps_num: Some(preset.video.fps),
        fps_den: Some(1),
        bitrate_video: Some(preset.video.bitrate_mbps * 1_000_000),
        bitrate_audio: Some(preset.audio.bitrate_kbps * 1000),
        storage_root: storage_root.to_string_lossy().into(),
        status: SessionStatus::Recording,
    })?;

    // Stop automatically when the game's window disappears (spec §20 game exit).
    spawn_exit_watcher(pid, exe.clone(), stop.clone());

    let params = RecordParams {
        session_id: session_id.clone(),
        game_slug: slug,
        session_start,
        output_dir,
        db_path: db_path.clone(),
        hwnd,
        pid,
        preset,
    };
    let result = record(params, stop.clone(), tx);

    let status = if result.is_ok() {
        SessionStatus::Stopped
    } else {
        SessionStatus::Error
    };
    let _ = store.end_session(&session_id, now_ms(), status);
    result
}

/// A currently-running app the user can register from.
#[derive(Debug, Clone)]
pub struct RunningApp {
    pub name: String,
    pub exe_path: String,
    pub process_name: String,
}

/// System/shell windows to hide from the "register from running apps" list.
fn is_system_process(pn: &str) -> bool {
    matches!(
        pn.to_lowercase().as_str(),
        "explorer.exe"
            | "applicationframehost.exe"
            | "textinputhost.exe"
            | "systemsettings.exe"
            | "shellexperiencehost.exe"
            | "searchhost.exe"
            | "startmenuexperiencehost.exe"
            | "launcher.exe"
            | "dwm.exe"
    )
}

/// Visible top-level apps with a window, deduped by executable.
pub fn list_running_apps() -> Vec<RunningApp> {
    let self_pid = std::process::id();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for w in window_detect::enumerate() {
        if !w.visible || w.title.is_empty() || w.pid == self_pid || is_noncapturable_class(&w.class) {
            continue;
        }
        if window_area(w.hwnd) <= 0 {
            continue;
        }
        let Some(path) = w.image_path.clone() else { continue };
        let pn = w.process_name.clone().unwrap_or_default();
        if is_system_process(&pn) || !seen.insert(path.to_lowercase()) {
            continue;
        }
        out.push(RunningApp {
            name: w.title.clone(),
            exe_path: path,
            process_name: pn,
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Lowercased process names of all currently-visible windows (auto-record probe).
pub fn running_process_names() -> std::collections::HashSet<String> {
    window_detect::enumerate()
        .iter()
        .filter(|w| w.visible)
        .filter_map(|w| w.process_name.as_ref().map(|p| p.to_lowercase()))
        .collect()
}

/// The `*.exe` file name from a registered launch command, lowercased.
pub fn exe_name(launch_command: &str) -> Option<String> {
    target_exe_name(launch_command)
}

/// The `*.exe` file name from a registered launch command, lowercased.
fn target_exe_name(launch_command: &str) -> Option<String> {
    let name = std::path::Path::new(launch_command)
        .file_name()?
        .to_string_lossy()
        .to_lowercase();
    name.ends_with(".exe").then_some(name)
}

/// Does this window match the game (by launched PID, exe name, or saved rule),
/// and is it a usable capture target?
fn matches_target(
    w: &matcher::WindowInfo,
    launched_pid: Option<u32>,
    exe: Option<&str>,
    rule: Option<&WindowRule>,
) -> bool {
    if !w.visible || is_noncapturable_class(&w.class) || window_area(w.hwnd) <= 0 {
        return false;
    }
    if launched_pid == Some(w.pid) {
        return true;
    }
    if let (Some(exe), Some(pn)) = (exe, &w.process_name) {
        if pn.to_lowercase() == exe {
            return true;
        }
    }
    rule.is_some_and(|r| matcher::match_tier(r, w).is_some())
}

/// Poll for the target window, then confirm it is *stable* (~1.5s) before
/// committing — skipping the transient bootstrapper window of games that exit
/// and relaunch (e.g. Shadowverse).
fn wait_for_target(
    launched_pid: Option<u32>,
    exe: Option<&str>,
    rule: Option<&WindowRule>,
    stop: &Arc<AtomicBool>,
    timeout: Duration,
) -> Option<(i64, u32, Option<String>)> {
    let pick = |wins: &[matcher::WindowInfo]| -> Option<(i64, u32, Option<String>)> {
        wins.iter()
            .filter(|w| matches_target(w, launched_pid, exe, rule))
            .max_by_key(|w| window_area(w.hwnd))
            .map(|w| (w.hwnd, w.pid, w.process_name.clone()))
    };

    let start = Instant::now();
    let mut waited_log = false;
    while start.elapsed() < timeout && !stop.load(Ordering::Relaxed) {
        let windows = window_detect::enumerate();
        if let Some((hwnd, pid, _)) = pick(&windows) {
            if let Some(w) = windows.iter().find(|w| w.hwnd == hwnd) {
                tracing::info!(hwnd, pid, title = %w.title, class = %w.class, image = ?w.image_path, "candidate window; confirming stability");
            }
            // Confirm a matching window still exists after a moment.
            std::thread::sleep(Duration::from_millis(1500));
            let windows2 = window_detect::enumerate();
            if let Some(committed) = pick(&windows2) {
                return Some(committed);
            }
            tracing::info!("candidate vanished (bootstrapper handoff?); retrying");
        } else if !waited_log {
            waited_log = true;
            tracing::info!(?launched_pid, ?exe, "no matching window yet; waiting");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

/// Window classes WGC's CreateForWindow rejects or that aren't the game itself.
fn is_noncapturable_class(class: &str) -> bool {
    matches!(
        class,
        "ConsoleWindowClass"
            | "PseudoConsoleWindow"
            | "CASCADIA_HOSTING_WINDOW_CLASS"
            | "Windows.UI.Core.CoreWindow"
    )
}

/// Window area in pixels via GetWindowRect (0 if unavailable).
fn window_area(hwnd: i64) -> i64 {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let mut r = RECT::default();
    if unsafe { GetWindowRect(HWND(hwnd as *mut core::ffi::c_void), &mut r) }.is_ok() {
        (r.right - r.left).max(0) as i64 * (r.bottom - r.top).max(0) as i64
    } else {
        0
    }
}

fn spawn_exit_watcher(pid: u32, exe: Option<String>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let exe = exe.map(|e| e.to_lowercase());
        // Generous grace so a splash->game handoff (window closes/reopens) doesn't
        // look like an exit.
        std::thread::sleep(Duration::from_secs(6));
        let mut misses = 0u32;
        while !stop.load(Ordering::Relaxed) {
            // Alive if any visible window matches the captured PID OR the game exe
            // (tolerates the game relaunching under a new PID).
            let alive = window_detect::enumerate().iter().any(|w| {
                w.visible
                    && (w.pid == pid
                        || matches!((&exe, &w.process_name), (Some(e), Some(pn)) if &pn.to_lowercase() == e))
            });
            // Require sustained absence (~3.5s) before declaring the game exited.
            misses = if alive { 0 } else { misses + 1 };
            if misses >= 5 {
                tracing::info!(pid, "game window gone; stopping recording");
                stop.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(Duration::from_millis(700));
        }
    });
}

fn timestamp() -> String {
    use time::OffsetDateTime;
    let now = OffsetDateTime::now_utc();
    let fmt = time::macros::format_description!(
        "[year][month][day]_[hour][minute][second]"
    );
    now.format(&fmt).unwrap_or_else(|_| now_ms().to_string())
}
