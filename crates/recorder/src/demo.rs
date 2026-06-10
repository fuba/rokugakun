//! End-to-end encode+save demo (validates the goal: a simple synthetic
//! video+audio source is correctly HEVC/AAC-encoded and written to a `.ts`).
//!
//! Generates animated NV12 frames + a sine tone, runs them through the **real**
//! NVENC HEVC and MF AAC encoders, merges packets by DTS, and muxes to MPEG-TS
//! via the segment writer. No window/loopback needed — those are validated by
//! their own `--ignored` tests; this proves the encode→mux→file path.

use crate::audio::{AudioFrame, LoopbackCapture};
use crate::capture::{D3dDevice, FrameGrid, GridFrame, Nv12Converter, WgcCapture};
use crate::encode::aac_mf::MfAacEncoder;
use crate::encode::hevc_mf::MfHevcEncoder;
use crate::encode::{AudioEncoder, AudioEncoderConfig, VideoEncoder, VideoEncoderConfig};
use crate::mux::{EncodedPacket, SegmentParams, SegmentWriter};
use anyhow::{anyhow, Result};
use rec_core::domain::{Game, Session, SessionStatus};
use rec_core::store::Store;
use rec_core::timebase::samples_to_100ns;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use ts_mux::StreamConfig;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const FPS: u32 = 60;
const SR: u32 = 48_000;

/// Run the demo, writing segment(s) under `out_dir`. Returns the first `.ts`.
pub fn run(out_dir: &Path, seconds: u32) -> Result<PathBuf> {
    crate::win::init_mta();
    crate::win::init_mf();
    std::fs::create_dir_all(out_dir)?;

    let d3d = D3dDevice::create()?;

    // ---- video: animated NV12 -> HEVC ----
    let vcfg = VideoEncoderConfig {
        width: WIDTH,
        height: HEIGHT,
        fps_num: FPS,
        fps_den: 1,
        bitrate_bps: 20_000_000,
        gop: 120,
        cbr: true,
    };
    let mut venc = MfHevcEncoder::new(vcfg, &d3d.device)?;

    let frame_interval = 10_000_000i64 / FPS as i64;
    let mut vpackets: Vec<EncodedPacket> = Vec::new();
    for i in 0..(seconds * FPS) {
        let texture = make_nv12_frame(&d3d, i)?;
        let frame = GridFrame {
            texture,
            pts_100ns: i as i64 * frame_interval,
            width: WIDTH,
            height: HEIGHT,
        };
        venc.encode(frame, &mut vpackets)?;
    }
    venc.flush(&mut vpackets)?;
    let codec_private = venc.codec_private();
    tracing::info!(
        video_packets = vpackets.len(),
        codec_private = codec_private.len(),
        "video encoded"
    );

    // ---- audio: 440Hz tone -> AAC ----
    let acfg = AudioEncoderConfig { sample_rate: SR, channels: 2, bitrate_bps: 192_000 };
    let mut aenc = MfAacEncoder::new(acfg)?;
    let mut apackets: Vec<EncodedPacket> = Vec::new();
    let total = seconds * SR;
    let mut n = 0u32;
    while n < total {
        let chunk = 480.min(total - n); // 10ms
        let mut samples = Vec::with_capacity(chunk as usize * 2);
        for j in 0..chunk {
            let s = (2.0 * std::f32::consts::PI * 440.0 * (n + j) as f32 / SR as f32).sin() * 0.3;
            samples.push(s);
            samples.push(s);
        }
        aenc.encode(
            AudioFrame { samples, sample_rate: SR, channels: 2, time_100ns: samples_to_100ns(n as i64, SR) },
            &mut apackets,
        )?;
        n += chunk;
    }
    aenc.flush(&mut apackets)?;
    tracing::info!(audio_packets = apackets.len(), "audio encoded");

    if vpackets.is_empty() {
        return Err(anyhow!("no video packets produced"));
    }
    if apackets.is_empty() {
        return Err(anyhow!("no audio packets produced"));
    }

    // ---- merge by DTS and mux ----
    let mut all: Vec<EncodedPacket> = vpackets;
    all.extend(apackets);
    all.sort_by_key(|p| p.dts_90k);

    let store = seeded_store(out_dir)?;
    let stream_cfg = StreamConfig {
        hevc_vps_sps_pps: codec_private,
        aac_sample_rate: SR,
        aac_channels: 2,
    };
    let params = SegmentParams {
        session_id: "demo".into(),
        game_slug: "Demo".into(),
        session_start: "test".into(),
        output_dir: out_dir.to_path_buf(),
        max_size_bytes: 1 << 30,
        max_duration_sec: 3600,
    };
    let mut writer = SegmentWriter::new(&store, stream_cfg, params);
    for pkt in &all {
        writer.write_packet(pkt)?;
    }
    writer.finish()?;

    let segs = store.list_segments("demo")?;
    let first = segs
        .first()
        .ok_or_else(|| anyhow!("no segment written"))?;
    Ok(PathBuf::from(&first.path))
}

/// Build a CPU-filled NV12 texture with a frame-dependent gradient.
fn make_nv12_frame(d3d: &D3dDevice, frame: u32) -> Result<ID3D11Texture2D> {
    let y_size = (WIDTH * HEIGHT) as usize;
    let uv_size = y_size / 2;
    let mut data = vec![0u8; y_size + uv_size];

    // Y plane: moving horizontal gradient.
    for row in 0..HEIGHT as usize {
        let base = row * WIDTH as usize;
        for col in 0..WIDTH as usize {
            data[base + col] = ((col + (frame as usize * 4)) & 0xFF) as u8;
        }
    }
    // UV plane: slowly shifting chroma so it's clearly "video".
    for px in data[y_size..].iter_mut() {
        *px = 128u8.wrapping_add((frame as u8).wrapping_mul(2));
    }

    let desc = D3D11_TEXTURE2D_DESC {
        Width: WIDTH,
        Height: HEIGHT,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let srd = D3D11_SUBRESOURCE_DATA {
        pSysMem: data.as_ptr() as *const _,
        SysMemPitch: WIDTH,
        SysMemSlicePitch: 0,
    };
    let mut tex: Option<ID3D11Texture2D> = None;
    unsafe {
        d3d.device.CreateTexture2D(&desc, Some(&srd), Some(&mut tex))?;
    }
    tex.ok_or_else(|| anyhow!("CreateTexture2D returned null"))
}

/// Live end-to-end: capture a real app's window (WGC) + its audio (process
/// loopback), encode HEVC+AAC, mux to `.ts`. Uses `ffplay` rendering a test
/// pattern + 440Hz tone as the "simple test app that outputs video and audio".
pub fn run_live(out_dir: &Path, seconds: u32) -> Result<PathBuf> {
    crate::win::init_mta();
    crate::win::init_mf();
    std::fs::create_dir_all(out_dir)?;

    // 1. Launch the test source app (ffplay window + tone).
    // Use the real exe (not the scoop shim, which spawns ffplay as a child whose
    // window PID would differ from child.id()).
    let ffplay = std::env::var("USERPROFILE")
        .map(|p| format!(r"{p}\scoop\apps\ffmpeg\current\bin\ffplay.exe"))
        .unwrap_or_else(|_| "ffplay".into());
    let graph = "testsrc2=size=1280x720:rate=60[out0];sine=frequency=440:sample_rate=48000[out1]";
    let mut child = std::process::Command::new(&ffplay)
        // -x/-y pin the window size so WGC's frame pool isn't invalidated by a resize.
        .args(["-loglevel", "quiet", "-x", "1280", "-y", "720", "-f", "lavfi", graph])
        .spawn()
        .map_err(|e| anyhow!("failed to launch ffplay ({ffplay}): {e}"))?;
    let pid = child.id();
    tracing::info!(pid, "launched ffplay test source");

    let result = (|| -> Result<PathBuf> {
        // 2. Wait for its window.
        let (hwnd, _, _) = wait_for_window(pid, Duration::from_secs(8))
            .ok_or_else(|| anyhow!("ffplay window not found"))?;
        // Let the window finish painting/sizing before we attach capture.
        std::thread::sleep(Duration::from_millis(900));
        let (_, in_w, in_h) =
            find_window_for_pid(pid).ok_or_else(|| anyhow!("window vanished"))?;
        tracing::info!(?hwnd, in_w, in_h, "found test window");

        // 3. Capture pipeline.
        let d3d = D3dDevice::create()?;
        let winrt = d3d.to_winrt()?;
        // Capture the monitor showing the test app: WGC delivers continuous frames
        // for monitors, whereas SDL/ffplay's window doesn't signal composition
        // updates to a per-window capture. (Per-window capture works for normal
        // DWM-composited apps — this is an ffplay-specific quirk.)
        let monitor = unsafe {
            windows::Win32::Graphics::Gdi::MonitorFromWindow(
                hwnd,
                windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTOPRIMARY,
            )
        };
        let wgc = WgcCapture::for_monitor(monitor, &winrt)?;
        let frames = wgc.frames();
        let loopback = LoopbackCapture::for_process(pid, SR, 2)?;

        let vcfg = VideoEncoderConfig {
            width: WIDTH,
            height: HEIGHT,
            fps_num: FPS,
            fps_den: 1,
            bitrate_bps: 20_000_000,
            gop: 120,
            cbr: true,
        };
        let mut venc = MfHevcEncoder::new(vcfg, &d3d.device)?;
        // Created lazily once we know the real frame size (monitor resolution).
        let mut converter: Option<Nv12Converter> = None;
        let mut aenc = MfAacEncoder::new(AudioEncoderConfig {
            sample_rate: SR,
            channels: 2,
            bitrate_bps: 192_000,
        })?;

        let mut grid = FrameGrid::default();
        let mut vpackets: Vec<EncodedPacket> = Vec::new();
        let mut apackets: Vec<EncodedPacket> = Vec::new();
        let mut audio_buf: Vec<AudioFrame> = Vec::new();
        let mut base: Option<i64> = None;

        let mut raw_count = 0u32;
        let mut fed = 0u32;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(seconds as u64) {
            // video
            while let Ok(raw) = frames.try_recv() {
                raw_count += 1;
                base.get_or_insert(raw.time_100ns);
                let conv = match &converter {
                    Some(c) => c,
                    None => {
                        converter = Some(Nv12Converter::new(&d3d, raw.width, raw.height, WIDTH, HEIGHT, None)?);
                        converter.as_ref().unwrap()
                    }
                };
                if let Some(dec) = grid.tick(raw.time_100ns) {
                    fed += 1;
                    let nv12 = conv.convert(&raw.texture)?;
                    venc.encode(
                        GridFrame { texture: nv12, pts_100ns: dec.pts_100ns, width: WIDTH, height: HEIGHT },
                        &mut vpackets,
                    )?;
                }
            }
            // audio
            loopback.poll(&mut audio_buf)?;
            if let Some(base) = base {
                for mut f in audio_buf.drain(..) {
                    f.time_100ns = (f.time_100ns - base).max(0);
                    aenc.encode(f, &mut apackets)?;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        venc.flush(&mut vpackets)?;
        aenc.flush(&mut apackets)?;
        let codec_private = venc.codec_private();
        tracing::info!(
            raw_frames = raw_count,
            fed_to_encoder = fed,
            video = vpackets.len(),
            audio = apackets.len(),
            "live capture encoded"
        );

        if vpackets.is_empty() {
            return Err(anyhow!("no video captured"));
        }

        // mux
        let mut all = vpackets;
        all.extend(apackets);
        all.sort_by_key(|p| p.dts_90k);
        let store = seeded_store(out_dir)?;
        let stream_cfg = StreamConfig {
            hevc_vps_sps_pps: codec_private,
            aac_sample_rate: SR,
            aac_channels: 2,
        };
        let params = SegmentParams {
            session_id: "demo".into(),
            game_slug: "Live".into(),
            session_start: "test".into(),
            output_dir: out_dir.to_path_buf(),
            max_size_bytes: 1 << 30,
            max_duration_sec: 3600,
        };
        let mut writer = SegmentWriter::new(&store, stream_cfg, params);
        for pkt in &all {
            writer.write_packet(pkt)?;
        }
        writer.finish()?;
        let segs = store.list_segments("demo")?;
        Ok(PathBuf::from(&segs.first().ok_or_else(|| anyhow!("no segment"))?.path))
    })();

    let _ = child.kill();
    let _ = child.wait();
    result
}

/// Probe per-window WGC capture: find a visible window whose title contains
/// `title_substr`, capture it for `secs`, and return how many frames arrived.
pub fn window_capture_test(title_substr: &str, secs: u32) -> Result<u32> {
    crate::win::init_mta();
    let hwnd = find_window_by_title(title_substr)
        .ok_or_else(|| anyhow!("no visible window containing '{title_substr}'"))?;
    tracing::info!(?hwnd, "capturing window per-HWND");

    let d3d = D3dDevice::create()?;
    let winrt = d3d.to_winrt()?;
    let wgc = WgcCapture::for_hwnd(hwnd, &winrt)?;
    let frames = wgc.frames();

    let mut count = 0u32;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs as u64) {
        while frames.try_recv().is_ok() {
            count += 1;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(count)
}

fn find_window_by_title(substr: &str) -> Option<HWND> {
    let mut hwnds: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut hwnds as *mut _ as isize));
    }
    let target = substr.to_lowercase();
    for hwnd in hwnds {
        if !unsafe { IsWindowVisible(hwnd).as_bool() } {
            continue;
        }
        let mut buf = [0u16; 512];
        let len = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf)
        };
        let title = String::from_utf16_lossy(&buf[..len.max(0) as usize]).to_lowercase();
        if !title.is_empty() && title.contains(&target) {
            return Some(hwnd);
        }
    }
    None
}

/// Poll for the target process's main visible window. Returns (hwnd, w, h).
fn wait_for_window(pid: u32, timeout: Duration) -> Option<(HWND, u32, u32)> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(found) = find_window_for_pid(pid) {
            return Some(found);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

fn find_window_for_pid(pid: u32) -> Option<(HWND, u32, u32)> {
    let mut hwnds: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut hwnds as *mut _ as isize));
    }
    let mut best: Option<(HWND, u32, u32)> = None;
    for hwnd in hwnds {
        let mut wpid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut wpid)) };
        if wpid != pid {
            continue;
        }
        if !unsafe { IsWindowVisible(hwnd).as_bool() } {
            continue;
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
            continue;
        }
        let (w, h) = ((rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32);
        if w == 0 || h == 0 {
            continue;
        }
        // pick the largest visible window for this pid
        if best.is_none_or(|(_, bw, bh)| w * h > bw * bh) {
            best = Some((hwnd, w, h));
        }
    }
    best
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let vec = &mut *(lparam.0 as *mut Vec<HWND>);
    vec.push(hwnd);
    BOOL(1)
}

fn seeded_store(out_dir: &Path) -> Result<Store> {
    let store = Store::open(&out_dir.join("demo.db"))?;
    store.upsert_game(&Game {
        id: "demo".into(),
        name: "Demo".into(),
        launch_command: "demo".into(),
        launch_workdir: None,
        launch_args: None,
        auto_record: false,
        preset: None,
        created_at: 0,
        updated_at: 0,
    })?;
    store.insert_session(&Session {
        id: "demo".into(),
        game_id: "demo".into(),
        started_at: 0,
        ended_at: None,
        codec_video: "hevc".into(),
        codec_audio: "aac".into(),
        container: "mpegts".into(),
        width: Some(WIDTH as i32),
        height: Some(HEIGHT as i32),
        fps_num: Some(FPS as i32),
        fps_den: Some(1),
        bitrate_video: Some(20_000_000),
        bitrate_audio: Some(192_000),
        storage_root: out_dir.to_string_lossy().into(),
        status: SessionStatus::Recording,
    })?;
    Ok(store)
}
