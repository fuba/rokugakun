//! egui launcher UI (spec §19): game list, "record & launch", live recording
//! screen. The recording engine runs in-process on a worker thread.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use rec_core::config::{AppConfig, AppPaths};
use rec_core::domain::{Game, WindowRule};
use rec_core::fsutil::now_ms;
use rec_core::preset::{EncoderBackend, RateControl, RecordingPreset};
use rec_core::protocol::RecorderMsg;
use rec_core::store::{RecentSession, Store};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use launcher::server;
#[cfg(windows)]
use launcher::session::{self, Recording};

fn main() -> eframe::Result<()> {
    let log_paths = AppPaths::default_layout();
    let _log_guard = rec_core::logging::init_with_file(&log_paths.logs_dir);
    tracing::info!(logs = %log_paths.logs_dir.display(), "launcher starting");

    // Headless end-to-end self-test (no GUI): `launcher selftest [seconds]`.
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("serve") {
            let paths = AppPaths::default_layout();
            let cfg = AppConfig::load_or_default(&paths.config_path).unwrap_or_default();
            let s = server::start(paths.db_path, cfg.storage_root, cfg.screenshot_dir, ffmpeg_path())
                .expect("server start");
            println!("serving: {}  (LAN: {})", s.url(), s.lan_url().unwrap_or_default());
            std::thread::sleep(std::time::Duration::from_secs(args.get(2).and_then(|x| x.parse().ok()).unwrap_or(600)));
            std::process::exit(0);
        }
        if args.get(1).map(String::as_str) == Some("list-apps") {
            for app in launcher::session::list_running_apps() {
                println!("{:<24} {}", app.process_name, app.name);
            }
            std::process::exit(0);
        }
        if args.get(1).map(String::as_str) == Some("selftest") {
            let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            std::process::exit(match run_selftest(secs) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("selftest failed: {e}");
                    1
                }
            });
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([780.0, 620.0])
            .with_min_inner_size([560.0, 420.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "rokugakun — game auto-recorder",
        options,
        Box::new(|cc| {
            install_japanese_fonts(&cc.egui_ctx);
            apply_theme(&cc.egui_ctx);
            Ok(Box::new(LauncherApp::new()))
        }),
    )
}

/// Window/taskbar icon: a red recording dot, generated at runtime.
fn app_icon() -> egui::IconData {
    const S: usize = 64;
    let mut rgba = vec![0u8; S * S * 4];
    let c = (S as f32 - 1.0) / 2.0;
    let r = S as f32 * 0.40;
    for y in 0..S {
        for x in 0..S {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            // anti-aliased edge: full inside, fading out over ~1.5px
            let cov = ((r - d) / 1.5 + 0.5).clamp(0.0, 1.0);
            let i = (y * S + x) * 4;
            rgba[i] = 225;
            rgba[i + 1] = 56;
            rgba[i + 2] = 56;
            rgba[i + 3] = (cov * 255.0) as u8;
        }
    }
    egui::IconData { rgba, width: S as u32, height: S as u32 }
}

const ACCENT: egui::Color32 = egui::Color32::from_rgb(0xE1, 0x4A, 0x4A);

/// Dark theme tuned for the launcher: rounded corners, roomier spacing,
/// slightly larger text, red accent matching the recording dot.
fn apply_theme(ctx: &egui::Context) {
    use egui::{Color32, FontId, Rounding, TextStyle};
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(19.0)),
        (TextStyle::Body, FontId::proportional(14.5)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(11.5)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(11.0, 5.0);
    style.spacing.interact_size.y = 26.0;

    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(0x16, 0x18, 0x1d);
    v.window_fill = Color32::from_rgb(0x1d, 0x21, 0x28);
    v.extreme_bg_color = Color32::from_rgb(0x10, 0x12, 0x16);
    v.faint_bg_color = Color32::from_rgb(0x1d, 0x21, 0x28);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.hyperlink_color = Color32::from_rgb(0x8f, 0xd6, 0xff);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = Rounding::same(6.0);
    }
    v.widgets.inactive.bg_fill = Color32::from_rgb(0x2a, 0x2f, 0x3a);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x2a, 0x2f, 0x3a);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x35, 0x3c, 0x4a);
    v.window_rounding = Rounding::same(8.0);
    style.visuals = v;
    ctx.set_style(style);
}

/// A subtle card frame for list rows / sections.
fn card_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(0x1d, 0x21, 0x28))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(0x2b, 0x30, 0x3a)))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(12.0, 9.0))
}

/// egui's default fonts lack CJK glyphs, so Japanese text renders as tofu (□).
/// Load a Windows system Japanese font and make it the primary family.
fn install_japanese_fonts(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\YuGothM.ttc",  // Yu Gothic Medium
        r"C:\Windows\Fonts\YuGothR.ttc",  // Yu Gothic Regular
        r"C:\Windows\Fonts\meiryo.ttc",   // Meiryo
        r"C:\Windows\Fonts\msgothic.ttc", // MS Gothic (always present)
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("jp".to_owned(), egui::FontData::from_owned(bytes));
            // Prepend so JP glyphs are preferred, keeping fallbacks for the rest.
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().insert(0, "jp".to_owned());
            }
            ctx.set_fonts(fonts);
            tracing::info!(font = path, "loaded Japanese font");
            return;
        }
    }
    tracing::warn!("no Japanese system font found; text may render as tofu");
}

/// Drive a full session (launch ffplay -> find window -> record -> stop) headlessly.
#[cfg(windows)]
fn run_selftest(seconds: u64) -> anyhow::Result<()> {
    let paths = AppPaths::default_layout();
    paths.ensure_dirs()?;
    let storage_root = std::env::temp_dir().join("rokugakun_launcher_selftest");
    let _ = std::fs::remove_dir_all(&storage_root);
    std::fs::create_dir_all(&storage_root)?;

    let ffplay = std::env::var("USERPROFILE")
        .map(|p| format!(r"{p}\scoop\apps\ffmpeg\current\bin\ffplay.exe"))
        .unwrap_or_else(|_| "ffplay".into());
    let now = now_ms();
    let game = Game {
        id: "selftest".into(),
        name: "SelfTest".into(),
        launch_command: ffplay,
        launch_workdir: None,
        launch_args: Some(
            "-loglevel quiet -f lavfi \"testsrc2=size=1280x720:rate=60[out0];sine=frequency=440:sample_rate=48000[out1]\""
                .into(),
        ),
        auto_record: false,
        preset: None,
        created_at: now,
        updated_at: now,
    };

    let mut preset = RecordingPreset::default_1440p60();
    preset.video.width = 1280;
    preset.video.height = 720;
    preset.video.bitrate_mbps = 15;

    println!("launching + recording for {seconds}s...");
    let rec = session::start(game, preset, storage_root.clone(), paths.db_path.clone());
    let mut segments = 0u32;
    let deadline = std::time::Instant::now() + Duration::from_secs(seconds);
    while std::time::Instant::now() < deadline {
        while let Ok(msg) = rec.rx.try_recv() {
            match msg {
                RecorderMsg::Status { state, .. } => println!("  status: {state}"),
                RecorderMsg::SegmentClosed { path, size_bytes } => {
                    segments += 1;
                    println!("  segment closed: {path} ({size_bytes} bytes)");
                }
                RecorderMsg::Error { code, message } => {
                    anyhow::bail!("recorder error [{code}]: {message}")
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    rec.request_stop();
    if let Some(h) = rec.handle {
        let _ = h.join();
    }
    while let Ok(msg) = rec.rx.try_recv() {
        if let RecorderMsg::SegmentClosed { path, size_bytes } = msg {
            segments += 1;
            println!("  segment closed: {path} ({size_bytes} bytes)");
        }
    }

    // Find the produced .ts files on disk.
    let mut ts_files: Vec<PathBuf> = Vec::new();
    collect_ts(&storage_root, &mut ts_files);
    println!("done: {segments} segment(s) reported, {} .ts file(s) on disk", ts_files.len());
    for f in &ts_files {
        let len = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        println!("  -> {} ({len} bytes)", f.display());
    }
    if ts_files.is_empty() || ts_files.iter().all(|f| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0) == 0) {
        anyhow::bail!("no non-empty .ts segment was written");
    }
    Ok(())
}

#[cfg(windows)]
fn collect_ts(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_ts(&p, out);
            } else if p.extension().is_some_and(|x| x == "ts") {
                out.push(p);
            }
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Games,
    Recordings,
}

struct LauncherApp {
    store: Option<Store>,
    config: AppConfig,
    config_path: PathBuf,
    db_path: PathBuf,
    games: Vec<Game>,
    new_name: String,
    new_cmd: String,
    show_settings: bool,
    show_running_picker: bool,
    /// Game id pending delete confirmation.
    confirm_delete: Option<String>,
    /// Game id whose per-game quality settings are being edited.
    editing_game: Option<String>,
    editing_preset: RecordingPreset,
    /// Which main tab is shown.
    tab: Tab,
    /// All sessions across games, newest first (Recordings tab).
    recent: Vec<RecentSession>,
    /// Optional (game_id, game_name) filter for the Recordings tab.
    rec_filter: Option<(String, String)>,
    /// The embedded web/HLS server, started lazily on first "browser" click.
    viewer_server: Option<server::ViewerServer>,
    /// Blinking red recording indicator (top-right), live while recording.
    #[cfg(windows)]
    rec_indicator: Option<launcher::overlay::RecIndicator>,
    #[cfg(windows)]
    running_apps: Vec<session::RunningApp>,
    /// exe names already auto-triggered (re-armed when the app closes).
    auto_seen: HashSet<String>,
    last_auto_check: Instant,
    message: String,
    // live recording state
    #[cfg(windows)]
    recording: Option<Recording>,
    rec_state: String,
    rec_duration_ms: i64,
    rec_bytes: i64,
    rec_segments: u32,
}

impl LauncherApp {
    fn new() -> Self {
        let paths = AppPaths::default_layout();
        let _ = paths.ensure_dirs();
        let config = AppConfig::load_or_default(&paths.config_path).unwrap_or_default();
        let (store, message) = match Store::open(&paths.db_path) {
            Ok(s) => (Some(s), String::new()),
            Err(e) => (None, format!("Cannot open the database: {e}")),
        };
        let games = store
            .as_ref()
            .and_then(|s| s.list_games().ok())
            .unwrap_or_default();

        LauncherApp {
            store,
            config,
            config_path: paths.config_path,
            db_path: paths.db_path,
            games,
            new_name: String::new(),
            new_cmd: String::new(),
            show_settings: false,
            show_running_picker: false,
            confirm_delete: None,
            editing_game: None,
            editing_preset: RecordingPreset::default_1440p60(),
            tab: Tab::Games,
            recent: Vec::new(),
            rec_filter: None,
            viewer_server: None,
            #[cfg(windows)]
            rec_indicator: None,
            #[cfg(windows)]
            running_apps: Vec::new(),
            auto_seen: HashSet::new(),
            last_auto_check: Instant::now(),
            message,
            #[cfg(windows)]
            recording: None,
            rec_state: String::new(),
            rec_duration_ms: 0,
            rec_bytes: 0,
            rec_segments: 0,
        }
    }

    fn reload_games(&mut self) {
        if let Some(store) = &self.store {
            self.games = store.list_games().unwrap_or_default();
        }
    }

    /// Native folder picker for the recordings storage root; persists to config.
    fn pick_storage_folder(&mut self) {
        let start = if self.config.storage_root.is_dir() {
            self.config.storage_root.clone()
        } else {
            std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default()
        };
        if let Some(dir) = rfd::FileDialog::new().set_directory(start).pick_folder() {
            self.config.storage_root = dir;
            match self.config.save(&self.config_path) {
                Ok(()) => self.message = format!("Storage folder changed: {}", self.config.storage_root.display()),
                Err(e) => self.message = format!("Failed to save the storage folder: {e}"),
            }
        }
    }

    /// Native folder picker for the web-viewer screenshot directory; persists.
    fn pick_screenshot_folder(&mut self) {
        let start = if self.config.screenshot_dir.is_dir() {
            self.config.screenshot_dir.clone()
        } else {
            std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default()
        };
        if let Some(dir) = rfd::FileDialog::new().set_directory(start).pick_folder() {
            self.config.screenshot_dir = dir;
            match self.config.save(&self.config_path) {
                Ok(()) => {
                    self.message =
                        format!("Screenshot folder changed: {}", self.config.screenshot_dir.display())
                }
                Err(e) => self.message = format!("Failed to save the screenshot folder: {e}"),
            }
        }
    }

    /// Native file picker for a game executable; auto-fills the name from the file.
    fn pick_game_file(&mut self) {
        if let Some(file) = rfd::FileDialog::new()
            .add_filter("Executables / shortcuts", &["exe", "lnk", "url"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            if let Some(stem) = file.file_stem() {
                self.new_name = stem.to_string_lossy().into_owned();
            }
            self.new_cmd = file.to_string_lossy().into_owned();
        }
    }

    fn add_game(&mut self) {
        let name = self.new_name.trim();
        let cmd = self.new_cmd.trim();
        if name.is_empty() || cmd.is_empty() {
            self.message = "Enter a name and a launch command".into();
            return;
        }
        if let Some(store) = &self.store {
            let now = now_ms();
            let game = Game {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.into(),
                launch_command: cmd.into(),
                launch_workdir: None,
                launch_args: None,
                auto_record: false,
                preset: None,
                created_at: now,
                updated_at: now,
            };
            if let Err(e) = store.upsert_game(&game) {
                self.message = format!("Failed to add: {e}");
                return;
            }
            self.new_name.clear();
            self.new_cmd.clear();
        }
        self.reload_games();
    }

    #[cfg(windows)]
    fn poll_recording(&mut self) {
        let mut finished = false;
        if let Some(rec) = &self.recording {
            while let Ok(msg) = rec.rx.try_recv() {
                match msg {
                    RecorderMsg::Status { state, duration_ms, size_bytes } => {
                        self.rec_state = state;
                        if let Some(d) = duration_ms {
                            self.rec_duration_ms = d;
                        }
                        if let Some(b) = size_bytes {
                            self.rec_bytes = b;
                        }
                    }
                    RecorderMsg::SegmentClosed { .. } => self.rec_segments += 1,
                    RecorderMsg::Error { code, message } => {
                        self.message = format!("Recording error [{code}]: {message}");
                    }
                }
            }
            if rec.is_finished() {
                finished = true;
            }
        }
        if finished {
            let game = self.recording.as_ref().map(|r| r.game_name.clone()).unwrap_or_default();
            self.message = format!("Recording finished: {game} ({} segments)", self.rec_segments);
            self.recording = None;
            self.reload_games();
        }
    }

    #[cfg(windows)]
    fn start_recording(&mut self, game: Game) {
        self.rec_state = "starting".into();
        self.rec_duration_ms = 0;
        self.rec_bytes = 0;
        self.rec_segments = 0;
        self.message = format!("Launching {} and starting the recording…", game.name);
        let preset = game.preset.clone().unwrap_or_else(|| self.config.preset.clone());
        self.recording = Some(session::start(
            game,
            preset,
            self.config.storage_root.clone(),
            self.db_path.clone(),
        ));
    }

    /// Auto-record: detect when an `auto_record` game's process appears and attach.
    #[cfg(windows)]
    fn poll_auto_record(&mut self) {
        if self.recording.is_some() || self.last_auto_check.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_auto_check = Instant::now();
        let auto_games: Vec<Game> = self.games.iter().filter(|g| g.auto_record).cloned().collect();
        if auto_games.is_empty() {
            return;
        }
        let running = session::running_process_names();
        for g in auto_games {
            let Some(exe) = session::exe_name(&g.launch_command) else { continue };
            if running.contains(&exe) {
                // newly seen -> trigger once (re-armed when the app closes)
                if self.auto_seen.insert(exe.clone()) {
                    tracing::info!(game = %g.name, %exe, "auto-record: process detected; attaching");
                    self.message = format!("Auto-record started: {}", g.name);
                    self.rec_state = "starting".into();
                    self.rec_segments = 0;
                    self.rec_bytes = 0;
                    self.rec_duration_ms = 0;
                    let preset = g.preset.clone().unwrap_or_else(|| self.config.preset.clone());
                    self.recording = Some(session::start_attach(
                        g.clone(),
                        preset,
                        self.config.storage_root.clone(),
                        self.db_path.clone(),
                    ));
                    return;
                }
            } else {
                self.auto_seen.remove(&exe); // re-arm for next launch
            }
        }
    }

    /// Register a game from a currently-running app (auto-record enabled).
    #[cfg(windows)]
    fn register_from_app(&mut self, app: &session::RunningApp) {
        let Some(store) = &self.store else { return };
        let now = now_ms();
        let stem = std::path::Path::new(&app.exe_path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned());
        let name = if !app.name.trim().is_empty() {
            app.name.clone()
        } else {
            stem.unwrap_or_else(|| app.process_name.clone())
        };
        let game = Game {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            launch_command: app.exe_path.clone(),
            launch_workdir: None,
            launch_args: None,
            auto_record: true,
            preset: None,
            created_at: now,
            updated_at: now,
        };
        if let Err(e) = store.upsert_game(&game) {
            self.message = format!("Failed to register: {e}");
            return;
        }
        let _ = store.upsert_window_rule(&WindowRule {
            game_id: game.id.clone(),
            process_name: Some(app.process_name.clone()),
            confidence: 1,
            ..Default::default()
        });
        self.message = format!("Registered with auto-record ON: {}", game.name);
        self.show_running_picker = false;
        self.reload_games();
    }

    /// Toggle a game's auto-record flag and persist it.
    fn set_auto_record(&mut self, mut game: Game, on: bool) {
        game.auto_record = on;
        game.updated_at = now_ms();
        if let Some(store) = &self.store {
            let _ = store.upsert_game(&game);
        }
        self.reload_games();
    }
}

/// Renders editor widgets for every field of a [`RecordingPreset`].
fn preset_editor(ui: &mut egui::Ui, p: &mut RecordingPreset) {
    egui::Grid::new(egui::Id::new("preset_grid").with(ui.id()))
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Resolution");
            egui::ComboBox::from_id_source(ui.id().with("res"))
                .selected_text(format!("{}x{}", p.video.width, p.video.height))
                .show_ui(ui, |ui| {
                    for (w, h) in [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)] {
                        if ui
                            .selectable_label(p.video.width == w && p.video.height == h, format!("{w}x{h}"))
                            .clicked()
                        {
                            p.video.width = w;
                            p.video.height = h;
                        }
                    }
                });
            ui.end_row();

            ui.label("Frame rate");
            egui::ComboBox::from_id_source(ui.id().with("fps"))
                .selected_text(format!("{} fps", p.video.fps))
                .show_ui(ui, |ui| {
                    for f in [30, 60, 120] {
                        ui.selectable_value(&mut p.video.fps, f, format!("{f} fps"));
                    }
                });
            ui.end_row();

            ui.label("Video bitrate");
            ui.add(egui::DragValue::new(&mut p.video.bitrate_mbps).range(2..=300).suffix(" Mbps"));
            ui.end_row();

            ui.label("Rate control");
            egui::ComboBox::from_id_source(ui.id().with("rc"))
                .selected_text(if matches!(p.video.rate_control, RateControl::Cbr) { "CBR" } else { "VBR" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut p.video.rate_control, RateControl::Cbr, "CBR (predictable size)");
                    ui.selectable_value(&mut p.video.rate_control, RateControl::Vbr, "VBR (favors quality)");
                });
            ui.end_row();

            ui.label("Keyframe interval");
            ui.add(egui::DragValue::new(&mut p.video.keyframe_interval_sec).range(1..=10).suffix(" s"));
            ui.end_row();

            ui.label("Encoder");
            egui::ComboBox::from_id_source(ui.id().with("enc"))
                .selected_text(backend_label(p.video.backend))
                .show_ui(ui, |ui| {
                    for b in [EncoderBackend::Auto, EncoderBackend::MediaFoundation, EncoderBackend::Nvenc, EncoderBackend::Amf] {
                        ui.selectable_value(&mut p.video.backend, b, backend_label(b));
                    }
                });
            ui.end_row();

            ui.label("Audio sample rate");
            egui::ComboBox::from_id_source(ui.id().with("sr"))
                .selected_text(format!("{} Hz", p.audio.sample_rate))
                .show_ui(ui, |ui| {
                    for sr in [48_000u32, 44_100] {
                        ui.selectable_value(&mut p.audio.sample_rate, sr, format!("{sr} Hz"));
                    }
                });
            ui.end_row();

            ui.label("Audio channels");
            egui::ComboBox::from_id_source(ui.id().with("ch"))
                .selected_text(format!("{} ch", p.audio.channels))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut p.audio.channels, 2u16, "2 (stereo)");
                    ui.selectable_value(&mut p.audio.channels, 1u16, "1 (mono)");
                });
            ui.end_row();

            ui.label("Audio bitrate");
            ui.add(egui::DragValue::new(&mut p.audio.bitrate_kbps).range(96..=320).suffix(" kbps"));
            ui.end_row();

            ui.label("Max segment size");
            ui.add(egui::DragValue::new(&mut p.segment.max_size_mb).range(100..=8192).suffix(" MB"));
            ui.end_row();

            ui.label("Max segment length");
            ui.add(egui::DragValue::new(&mut p.segment.max_duration_sec).range(30..=3600).suffix(" s"));
            ui.end_row();

            ui.label("Total storage cap");
            ui.add(egui::DragValue::new(&mut p.retention.max_total_gb).range(10..=20000).suffix(" GB"));
            ui.end_row();
        });
}

/// Resolve the bundled ffplay (scoop) path, falling back to PATH.
fn ffplay_path() -> String {
    ff_tool("ffplay")
}
/// Resolve the bundled ffmpeg path, falling back to PATH.
fn ffmpeg_path() -> String {
    ff_tool("ffmpeg")
}
fn ff_tool(name: &str) -> String {
    // 1) next to the launcher exe (bundled distribution), 2) scoop, 3) PATH.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(format!("{name}.exe"));
            if p.exists() {
                return p.to_string_lossy().into_owned();
            }
        }
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        let p = format!(r"{home}\scoop\apps\ffmpeg\current\bin\{name}.exe");
        if std::path::Path::new(&p).exists() {
            return p;
        }
    }
    name.into()
}

fn local_offset() -> time::UtcOffset {
    static OFF: std::sync::OnceLock<time::UtcOffset> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC))
}

/// Format an epoch-ms timestamp as `MM/DD HH:MM` in local time.
fn fmt_ts(ms: i64) -> String {
    use time::OffsetDateTime;
    match OffsetDateTime::from_unix_timestamp(ms / 1000) {
        Ok(t) => {
            let t = t.to_offset(local_offset());
            format!(
                "{:02}/{:02} {:02}:{:02}",
                t.month() as u8,
                t.day(),
                t.hour(),
                t.minute()
            )
        }
        Err(_) => "----".into(),
    }
}

/// Format a duration in ms as `H:MM:SS` (or `M:SS`).
fn fmt_dur(ms: i64) -> String {
    let secs = (ms / 1000).max(0);
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn backend_label(b: EncoderBackend) -> &'static str {
    match b {
        EncoderBackend::Auto => "Auto",
        EncoderBackend::MediaFoundation => "Media Foundation",
        EncoderBackend::Nvenc => "NVENC",
        EncoderBackend::Amf => "AMF",
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(windows)]
        {
            self.poll_recording();
            self.poll_auto_record();
        }

        #[cfg(windows)]
        let is_recording = self.recording.is_some();
        #[cfg(not(windows))]
        let is_recording = false;

        egui::TopBottomPanel::top("topbar")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(0x1d, 0x21, 0x28))
                    .inner_margin(egui::Margin::symmetric(14.0, 9.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let color = if is_recording { ACCENT } else { egui::Color32::from_gray(110) };
                    ui.label(egui::RichText::new("●").color(color).size(17.0));
                    ui.heading("rokugakun");
                    ui.add_space(10.0);
                    if ui.selectable_label(self.tab == Tab::Games, "Games").clicked() {
                        self.tab = Tab::Games;
                    }
                    if ui.selectable_label(self.tab == Tab::Recordings, "Recordings").clicked() {
                        self.open_recordings(None);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("⚙ Settings").clicked() {
                            self.show_settings = !self.show_settings;
                        }
                        if ui.button("🌐 Web Viewer").on_hover_text("Open the browser viewer (seek bar / clip / screenshot)").clicked() {
                            self.open_browser("/");
                        }
                    });
                });
            });

        egui::TopBottomPanel::bottom("statusbar")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(0x12, 0x14, 0x18))
                    .inner_margin(egui::Margin::symmetric(14.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.message.is_empty() {
                        ui.weak("Ready");
                    } else {
                        ui.label(egui::RichText::new(&self.message).size(12.5));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(
                            egui::RichText::new(format!("Storage: {}", self.config.storage_root.display()))
                                .size(11.5),
                        );
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(14.0, 12.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if is_recording {
                        self.recording_screen(ui);
                    } else if self.editing_game.is_some() {
                        self.game_settings_editor(ui);
                    } else {
                        match self.tab {
                            Tab::Games => self.library_screen(ui),
                            Tab::Recordings => self.recordings_screen(ui),
                        }
                    }
                });
            });

        // Global settings as a floating window (toggled from the top bar).
        if self.show_settings {
            let mut open = true;
            egui::Window::new("Settings (global preset)")
                .open(&mut open)
                .default_width(400.0)
                .collapsible(false)
                .show(ctx, |ui| self.settings_section(ui));
            if !open {
                self.show_settings = false;
            }
        }

        // Blinking red "recording" dot (top-right), shown only while recording —
        // replaces the (removed) WGC yellow border.
        #[cfg(windows)]
        {
            if self.recording.is_some() && self.rec_indicator.is_none() {
                self.rec_indicator = Some(launcher::overlay::show());
            } else if self.recording.is_none() && self.rec_indicator.is_some() {
                self.rec_indicator = None; // Drop removes the dot
            }
        }

        // keep polling status while recording
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

impl LauncherApp {
    fn recording_screen(&mut self, ui: &mut egui::Ui) {
        #[cfg(windows)]
        {
            let name = self
                .recording
                .as_ref()
                .map(|r| r.game_name.clone())
                .unwrap_or_default();
            ui.add_space(8.0);
            card_frame(ui).show(ui, |ui| {
                ui.horizontal(|ui| {
                    // blink the dot ~1Hz in sync with the on-screen indicator
                    let on = (ui.input(|i| i.time) % 1.0) < 0.6;
                    let color = if on { ACCENT } else { ACCENT.gamma_multiply(0.35) };
                    ui.label(egui::RichText::new("●").color(color).size(22.0));
                    ui.heading(format!("Recording: {name}"));
                });
                ui.add_space(4.0);
                let secs = self.rec_duration_ms / 1000;
                ui.label(
                    egui::RichText::new(format!(
                        "{:02}:{:02}:{:02}",
                        secs / 3600,
                        (secs % 3600) / 60,
                        secs % 60
                    ))
                    .size(34.0)
                    .strong(),
                );
                ui.add_space(4.0);
                egui::Grid::new("rec_stats").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    ui.weak("State");
                    ui.label(&self.rec_state);
                    ui.end_row();
                    ui.weak("Segments");
                    ui.label(format!("{}", self.rec_segments));
                    ui.end_row();
                    ui.weak("Size");
                    ui.label(format!(
                        "{:.2} GB / cap {} GB",
                        self.rec_bytes as f64 / 1e9,
                        self.config.preset.retention.max_total_gb
                    ));
                    ui.end_row();
                });
                ui.add_space(8.0);
                let stop = egui::Button::new(
                    egui::RichText::new("■ Stop Recording").color(egui::Color32::WHITE),
                )
                .fill(ACCENT.gamma_multiply(0.8));
                if ui.add(stop).clicked() {
                    if let Some(rec) = &self.recording {
                        rec.request_stop();
                    }
                    self.message = "Stop requested…".into();
                }
                ui.weak("The small red dot at the top-right of your screen marks recording (it is never captured).");
            });
        }
    }

    /// Global recording/quality settings (spec §8).
    fn settings_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        preset_editor(ui, &mut self.config.preset);
        ui.add_space(8.0);

        ui.strong("Folders");
        egui::Grid::new("folders").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Recordings");
            ui.horizontal(|ui| {
                if ui.button("Change…").clicked() {
                    self.pick_storage_folder();
                }
                ui.weak(self.config.storage_root.display().to_string());
            });
            ui.end_row();
            ui.label("Screenshots");
            ui.horizontal(|ui| {
                if ui.button("Change…").clicked() {
                    self.pick_screenshot_folder();
                }
                ui.weak(self.config.screenshot_dir.display().to_string());
            });
            ui.end_row();
        });
        ui.weak("Screenshots taken in the web viewer are saved here on this PC.");

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                match self.config.save(&self.config_path) {
                    Ok(()) => self.message = "Settings saved".into(),
                    Err(e) => self.message = format!("Failed to save settings: {e}"),
                }
            }
            if ui.button("Reset to defaults").clicked() {
                self.config.preset = RecordingPreset::default_1440p60();
            }
        });
        ui.weak("B-frames are fixed at 0 for MPEG-TS integrity. Output auto-fits when the window is smaller than the preset.");
    }

    /// Per-game quality override editor.
    fn game_settings_editor(&mut self, ui: &mut egui::Ui) {
        let Some(gid) = self.editing_game.clone() else { return };
        let name = self
            .games
            .iter()
            .find(|g| g.id == gid)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                self.editing_game = None;
            }
            ui.heading(format!("Quality: {name}"));
        });
        ui.weak("Per-game override; falls back to the global settings unless saved.");
        ui.add_space(4.0);
        card_frame(ui).show(ui, |ui| preset_editor(ui, &mut self.editing_preset));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Save for this game").clicked() {
                self.apply_game_preset(&gid, Some(self.editing_preset.clone()));
                self.message = format!("Saved quality override for \"{name}\"");
                self.editing_game = None;
            }
            if ui.button("Use global settings").clicked() {
                self.apply_game_preset(&gid, None);
                self.message = format!("\"{name}\" now uses the global settings");
                self.editing_game = None;
            }
            if ui.button("Cancel").clicked() {
                self.editing_game = None;
            }
        });
    }

    fn apply_game_preset(&mut self, gid: &str, preset: Option<RecordingPreset>) {
        if let Some(mut g) = self.games.iter().find(|g| g.id == gid).cloned() {
            g.preset = preset;
            g.updated_at = now_ms();
            if let Some(store) = &self.store {
                let _ = store.upsert_game(&g);
            }
            self.reload_games();
        }
    }

    fn delete_game(&mut self, gid: &str) {
        if let Some(store) = &self.store {
            match store.delete_game(gid) {
                Ok(()) => {
                    self.message =
                        "Game removed (its recordings stay in the Recordings tab)".into()
                }
                Err(e) => self.message = format!("Delete failed: {e}"),
            }
        }
        self.reload_games();
    }

    /// Ensure the web server is running, then open `hash` (e.g. `/game/{id}`) in
    /// the browser.
    fn open_browser(&mut self, hash: &str) {
        if self.viewer_server.is_none() {
            match server::start(
                self.db_path.clone(),
                self.config.storage_root.clone(),
                self.config.screenshot_dir.clone(),
                ffmpeg_path(),
            ) {
                Ok(s) => self.viewer_server = Some(s),
                Err(e) => {
                    self.message = format!("Viewer server failed to start: {e}");
                    return;
                }
            }
        }
        if let Some(s) = &self.viewer_server {
            let url = format!("{}#{}", s.url(), hash);
            let _ = webbrowser::open(&url);
            let lan = s.lan_url().map(|u| format!("(LAN: {u})")).unwrap_or_default();
            self.message = format!("Opened in browser: {} {}", s.url(), lan);
        }
    }

    /// Switch to the Recordings tab, optionally filtered to one game.
    fn open_recordings(&mut self, filter: Option<(String, String)>) {
        self.recent = self
            .store
            .as_ref()
            .and_then(|s| s.recent_sessions(500).ok())
            .unwrap_or_default();
        self.rec_filter = filter;
        self.tab = Tab::Recordings;
    }

    /// Recordings tab: all sessions across games, newest first. Recordings of
    /// deleted (archived) games stay listed here.
    fn recordings_screen(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Recordings");
            if let Some((gid, name)) = self.rec_filter.clone() {
                if ui
                    .button(format!("Game: {name} ✕"))
                    .on_hover_text("Clear filter")
                    .clicked()
                {
                    self.rec_filter = None;
                }
                if ui.button("▶ Play all").on_hover_text("Play every session in a row").clicked() {
                    let paths = self
                        .store
                        .as_ref()
                        .and_then(|s| s.segment_paths_for_game(&gid).ok());
                    if let Some(paths) = paths {
                        self.play_segments(&format!("{name} (all sessions)"), paths);
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⟳ Refresh").clicked() {
                    let f = self.rec_filter.clone();
                    self.open_recordings(f);
                }
            });
        });
        ui.add_space(4.0);

        let sessions: Vec<RecentSession> = self
            .recent
            .iter()
            .filter(|r| {
                self.rec_filter
                    .as_ref()
                    .is_none_or(|(gid, _)| r.summary.session.game_id == *gid)
            })
            .cloned()
            .collect();

        if sessions.is_empty() {
            card_frame(ui).show(ui, |ui| {
                ui.weak("No recordings yet. Record a game from the Games tab.");
            });
            return;
        }

        let total_bytes: i64 = sessions.iter().map(|s| s.summary.total_bytes).sum();
        ui.weak(format!(
            "{} sessions ・ {:.2} GB total",
            sessions.len(),
            total_bytes as f64 / 1e9
        ));
        ui.add_space(2.0);

        for r in &sessions {
            let s = &r.summary;
            card_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.strong(egui::RichText::new(&r.game_name).size(15.0));
                    ui.label(fmt_ts(s.session.started_at));
                    ui.label(fmt_dur(s.duration_ms));
                    ui.weak(format!(
                        "{:.2} GB ・ {} files",
                        s.total_bytes as f64 / 1e9,
                        s.segment_count
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("🌐 Browser").on_hover_text("Watch in the browser").clicked() {
                            self.open_browser(&format!("/session/{}", s.session.id));
                        }
                        if ui.button("▶ Play").clicked() {
                            let paths = self
                                .store
                                .as_ref()
                                .and_then(|st| st.segment_paths_for_session(&s.session.id).ok());
                            if let Some(paths) = paths {
                                self.play_segments(
                                    &format!("{} {}", r.game_name, fmt_ts(s.session.started_at)),
                                    paths,
                                );
                            }
                        }
                    });
                });
            });
        }
    }

    /// Play an ordered list of `.ts` segments seamlessly via ffplay's concat
    /// protocol (TS is designed for byte-level concatenation).
    fn play_segments(&mut self, title: &str, paths: Vec<String>) {
        let existing: Vec<String> = paths
            .into_iter()
            .filter(|p| std::path::Path::new(p).exists())
            .collect();
        if existing.is_empty() {
            self.message = "No playable segments found".into();
            return;
        }
        let count = existing.len();
        let input = format!("concat:{}", existing.join("|"));
        match std::process::Command::new(ffplay_path())
            .args(["-loglevel", "warning", "-window_title", title, "-i"])
            .arg(&input)
            .spawn()
        {
            Ok(_) => self.message = format!("Playing: {title} ({count} files joined)"),
            Err(e) => self.message = format!("Playback failed (ffplay required): {e}"),
        }
    }

    fn library_screen(&mut self, ui: &mut egui::Ui) {
        // ---- registration card -------------------------------------------
        card_frame(ui).show(ui, |ui| {
            ui.strong("Add a game");
            ui.horizontal(|ui| {
                if ui.button("📁 Choose file...").clicked() {
                    self.pick_game_file();
                }
                #[cfg(windows)]
                if ui.button("⏵ From running apps...").clicked() {
                    self.show_running_picker = !self.show_running_picker;
                    if self.show_running_picker {
                        self.running_apps = session::list_running_apps();
                    }
                }
            });

            #[cfg(windows)]
            if self.show_running_picker {
                self.running_apps_picker(ui);
            }

            egui::Grid::new("add_game").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut self.new_name).desired_width(f32::INFINITY));
                ui.end_row();
                ui.label("Launch");
                ui.add(egui::TextEdit::singleline(&mut self.new_cmd).desired_width(f32::INFINITY));
                ui.end_row();
            });
            if ui.button("＋ Add").clicked() {
                self.add_game();
            }
        });

        ui.add_space(8.0);
        let v = self.config.preset.video.clone();
        ui.horizontal(|ui| {
            ui.strong("Games");
            ui.weak(format!(
                "(default quality: {}x{} @{}fps {}Mbps {})",
                v.width,
                v.height,
                v.fps,
                v.bitrate_mbps,
                if matches!(v.rate_control, RateControl::Cbr) { "CBR" } else { "VBR" }
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Change storage folder...").clicked() {
                    self.pick_storage_folder();
                }
            });
        });
        ui.add_space(2.0);

        let games = self.games.clone();
        if games.is_empty() {
            card_frame(ui).show(ui, |ui| {
                ui.weak("No games yet. Register an exe / .lnk / URL / shell:AppsFolder\\<AUMID>.");
            });
        }
        for game in games {
            card_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.strong(egui::RichText::new(&game.name).size(16.0));
                    if game.preset.is_some() {
                        ui.label(
                            egui::RichText::new("custom quality")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(0x8f, 0xd6, 0xff)),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut auto = game.auto_record;
                        if ui
                            .checkbox(&mut auto, "Auto-record")
                            .on_hover_text("Watch for this app and start recording automatically when it launches")
                            .changed()
                        {
                            self.set_auto_record(game.clone(), auto);
                        }
                    });
                });
                ui.small(egui::RichText::new(&game.launch_command).color(egui::Color32::from_gray(130)));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    #[cfg(windows)]
                    {
                        let rec = egui::Button::new(
                            egui::RichText::new("▶ Record & Launch").color(egui::Color32::WHITE),
                        )
                        .fill(ACCENT.gamma_multiply(0.75));
                        if ui.add(rec).clicked() {
                            self.start_recording(game.clone());
                        }
                    }
                    if ui.button("Recordings").clicked() {
                        self.open_recordings(Some((game.id.clone(), game.name.clone())));
                    }
                    if ui.button("Quality").clicked() {
                        self.editing_preset = game
                            .preset
                            .clone()
                            .unwrap_or_else(|| self.config.preset.clone());
                        self.editing_game = Some(game.id.clone());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.confirm_delete.as_deref() == Some(game.id.as_str()) {
                            if ui.button("Cancel").clicked() {
                                self.confirm_delete = None;
                            }
                            let del = egui::Button::new(
                                egui::RichText::new("Really delete").color(egui::Color32::WHITE),
                            )
                            .fill(ACCENT.gamma_multiply(0.8));
                            if ui.add(del).clicked() {
                                self.delete_game(&game.id);
                                self.confirm_delete = None;
                            }
                            ui.weak("recordings are kept —");
                        } else if ui.button("Delete").clicked() {
                            self.confirm_delete = Some(game.id.clone());
                        }
                    });
                });
            });
        }
    }

    /// List of currently-running apps with a "register" button each.
    #[cfg(windows)]
    fn running_apps_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Running apps (registered with auto-record ON):");
            if ui.button("Refresh").clicked() {
                self.running_apps = session::list_running_apps();
            }
        });
        let apps = self.running_apps.clone();
        egui::ScrollArea::vertical()
            .id_source("running_apps")
            .max_height(170.0)
            .show(ui, |ui| {
                if apps.is_empty() {
                    ui.weak("(no candidate apps found — press Refresh)");
                }
                for app in &apps {
                    ui.horizontal(|ui| {
                        if ui.button("Register").clicked() {
                            self.register_from_app(app);
                        }
                        ui.label(&app.name);
                    });
                    ui.small(&app.process_name);
                }
            });
        ui.separator();
    }
}
