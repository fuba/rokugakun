//! Stoppable live recording pipeline (spec §20/§23).
//!
//! Captures the monitor showing the target window (reliable continuous frames) +
//! the target process's audio, encodes HEVC+AAC, writes rotating `.ts` segments,
//! and runs retention. Driven by a stop flag; emits status via a channel.

use crate::audio::{AudioFrame, LoopbackCapture};
use crate::capture::{D3dDevice, FrameGrid, GridFrame, Nv12Converter, WgcCapture};
use crate::encode::aac_mf::MfAacEncoder;
use crate::encode::hevc_mf::MfHevcEncoder;
use crate::encode::{AudioEncoder, AudioEncoderConfig, VideoEncoder, VideoEncoderConfig};
use crate::mux::{EncodedPacket, SegmentParams, SegmentWriter};
use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use rec_core::preset::RecordingPreset;
use rec_core::protocol::RecorderMsg;
use rec_core::retention::RetentionManager;
use rec_core::store::Store;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use ts_mux::StreamConfig;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{ClientToScreen, MonitorFromWindow, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

/// Everything needed to run one recording session.
pub struct RecordParams {
    pub session_id: String,
    pub game_slug: String,
    pub session_start: String,
    pub output_dir: PathBuf,
    pub db_path: PathBuf,
    pub hwnd: i64,
    pub pid: u32,
    pub preset: RecordingPreset,
}

/// Run until `stop` is set. Writes segments + runs retention; reports via `status`.
pub fn record(params: RecordParams, stop: Arc<AtomicBool>, status: Sender<RecorderMsg>) -> Result<()> {
    crate::win::init_mta();
    crate::win::init_mf();

    let store = Store::open(&params.db_path)?;
    let p = &params.preset;

    // Capture the target window only (spec §9). If it yields no frames within a
    // couple of seconds (some apps present via a child surface WGC can't see per
    // window), fall back to capturing the whole monitor.
    let d3d = D3dDevice::create().context("D3D device")?;
    let winrt = d3d.to_winrt().context("WinRT device")?;
    let window_handle = HWND(params.hwnd as *mut core::ffi::c_void);
    let mut fallback_done = false;
    // Prefer per-window capture; if it can't attach (e.g. console/child-surface
    // windows reject CreateForWindow), fall back to whole-monitor capture.
    let mut wgc = match WgcCapture::for_hwnd(window_handle, &winrt) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "per-window capture init failed; using monitor");
            fallback_done = true;
            let monitor = unsafe { MonitorFromWindow(window_handle, MONITOR_DEFAULTTOPRIMARY) };
            WgcCapture::for_monitor(monitor, &winrt).context("monitor capture")?
        }
    };
    let mut frames = wgc.frames();
    let loopback = LoopbackCapture::for_process(params.pid, p.audio.sample_rate, p.audio.channels)
        .context("audio loopback")?;

    // For non-fullscreen windows, crop to the client area (exclude title bar /
    // borders). None for fullscreen/borderless or monitor capture.
    let mut client_crop = if fallback_done {
        None
    } else {
        compute_client_crop(window_handle)
    };

    // Effective source size: the cropped client area when cropping, else the full
    // capture. Don't upscale: cap output to that size (even). A window smaller
    // than the preset is recorded at its own size.
    let (eff_w, eff_h) = match &client_crop {
        Some(r) => ((r.right - r.left).max(2) as u32, (r.bottom - r.top).max(2) as u32),
        None => wgc.size(),
    };
    let (mut out_w, mut out_h) = output_size(p, eff_w, eff_h);
    tracing::info!(
        eff_w, eff_h, cropped = client_crop.is_some(),
        preset_w = p.video.width, preset_h = p.video.height, out_w, out_h,
        mode = ?p.video.resolution_mode,
        "capture source -> encoder output size"
    );

    let vcfg = VideoEncoderConfig {
        width: out_w,
        height: out_h,
        fps_num: p.video.fps as u32,
        fps_den: 1,
        bitrate_bps: (p.video.bitrate_mbps as u32) * 1_000_000,
        gop: p.gop_frames() as u32,
        cbr: matches!(p.video.rate_control, rec_core::preset::RateControl::Cbr),
    };
    let mut venc = MfHevcEncoder::new(vcfg, &d3d.device).context("HEVC encoder")?;
    let mut aenc = MfAacEncoder::new(AudioEncoderConfig {
        sample_rate: p.audio.sample_rate,
        channels: p.audio.channels,
        bitrate_bps: (p.audio.bitrate_kbps as u32) * 1000,
    })
    .context("AAC encoder")?;

    let stream_cfg = StreamConfig {
        hevc_vps_sps_pps: venc.codec_private(),
        aac_sample_rate: p.audio.sample_rate,
        aac_channels: p.audio.channels as u8,
    };
    let mut writer = SegmentWriter::new(
        &store,
        stream_cfg,
        SegmentParams {
            session_id: params.session_id.clone(),
            game_slug: params.game_slug.clone(),
            session_start: params.session_start.clone(),
            output_dir: params.output_dir.clone(),
            max_size_bytes: p.segment_max_bytes(),
            max_duration_sec: p.segment.max_duration_sec,
        },
    );
    let retention = RetentionManager::new(&store, p.retention_max_bytes());

    let mut converter: Option<Nv12Converter> = None;
    // Source size the current converter/encoder were built for; drives mid-session
    // resolution-change handling. `pending_src` debounces transient sizes (e.g.
    // while a window is being drag-resized) before committing a switch.
    let mut cur_src: Option<(u32, u32)> = None;
    let mut pending_src: Option<((u32, u32), Instant)> = None;
    const RES_DEBOUNCE: Duration = Duration::from_millis(400);
    let mut grid = FrameGrid::default();
    let mut audio_buf: Vec<AudioFrame> = Vec::new();
    let mut base: Option<i64> = None;
    let mut raw_count: u32 = 0;
    let mut vbuf: Vec<EncodedPacket> = Vec::new();
    let mut abuf: Vec<EncodedPacket> = Vec::new();
    let mut total_bytes: i64 = 0;
    let started = Instant::now();
    let mut last_status = Instant::now();

    let _ = status.send(RecorderMsg::Status {
        state: "capturing".into(),
        duration_ms: Some(0),
        size_bytes: Some(0),
    });

    while !stop.load(Ordering::Relaxed) {
        // video
        while let Ok(raw) = frames.try_recv() {
            raw_count += 1;
            base.get_or_insert(raw.time_100ns);
            let obs = (raw.width, raw.height);

            if cur_src.is_none() {
                // First frame: build the converter for this source size.
                let crop = if fallback_done { None } else { client_crop };
                converter = Some(
                    Nv12Converter::new(&d3d, raw.width, raw.height, out_w, out_h, crop)
                        .with_context(|| format!("NV12 converter {}x{}", raw.width, raw.height))?,
                );
                cur_src = Some(obs);
            } else if cur_src != Some(obs) {
                // Source resolution changed mid-session. Debounce transient sizes,
                // then switch the encoder + segment (TS carries the new params).
                let stable = match pending_src {
                    Some((s, since)) if s == obs => since.elapsed() >= RES_DEBOUNCE,
                    _ => {
                        pending_src = Some((obs, Instant::now()));
                        false
                    }
                };
                if !stable {
                    continue; // skip transitional frames until the size settles
                }
                pending_src = None;

                // New crop + output size (capped to the preset, never upscaled).
                let new_crop = if fallback_done { None } else { compute_client_crop(window_handle) };
                let (e_w, e_h) = match &new_crop {
                    Some(r) => ((r.right - r.left).max(2) as u32, (r.bottom - r.top).max(2) as u32),
                    None => obs,
                };
                let (nw, nh) = output_size(p, e_w, e_h);

                if (nw, nh) == (out_w, out_h) {
                    // Output size unchanged (e.g. a windowed resize still capped by
                    // the preset): just rebuild the converter; keep the segment.
                    converter = Some(Nv12Converter::new(&d3d, obs.0, obs.1, out_w, out_h, new_crop)?);
                    client_crop = new_crop;
                    cur_src = Some(obs);
                } else {
                    tracing::info!(
                        from_w = out_w, from_h = out_h, to_w = nw, to_h = nh,
                        src_w = obs.0, src_h = obs.1,
                        "resolution changed; switching encoder + segment"
                    );
                    total_bytes += switch_encoder_resolution(
                        p, &d3d, nw, nh, &mut venc, &mut writer, &mut vbuf, &mut abuf, &status,
                    )?;
                    // Rebuild the converter for the new source→output mapping.
                    converter = Some(Nv12Converter::new(&d3d, obs.0, obs.1, nw, nh, new_crop)?);
                    client_crop = new_crop;
                    out_w = nw;
                    out_h = nh;
                    cur_src = Some(obs);
                }
            } else {
                pending_src = None; // size matches; cancel any pending switch
            }

            let conv = converter.as_ref().unwrap();
            if let Some(dec) = grid.tick(raw.time_100ns) {
                let nv12 = conv.convert(&raw.texture).context("NV12 convert")?;
                venc.encode(
                    GridFrame { texture: nv12, pts_100ns: dec.pts_100ns, width: out_w, height: out_h },
                    &mut vbuf,
                )
                .context("HEVC encode")?;
            }
        }
        // If per-window capture produced nothing, switch to whole-monitor capture.
        if !fallback_done && raw_count < 3 && started.elapsed() > Duration::from_secs(2) {
            fallback_done = true;
            tracing::warn!("per-window capture delivered no frames; falling back to monitor");
            let monitor = unsafe { MonitorFromWindow(window_handle, MONITOR_DEFAULTTOPRIMARY) };
            if let Ok(mon) = WgcCapture::for_monitor(monitor, &winrt) {
                wgc = mon;
                frames = wgc.frames();
                converter = None; // frame size changes
                cur_src = None; // rebuild converter for the monitor source size
                pending_src = None;
                client_crop = None; // no client-area crop on the monitor fallback

                // The monitor is (almost always) far larger than the splash/window
                // we first sized the encoder for — e.g. Forza's tiny launcher
                // splash. Recompute the output size from the monitor and rebuild
                // the encoder, so we don't squash the whole desktop into the tiny
                // initial resolution and then never recover.
                let (mw, mh) = wgc.size();
                let (nw, nh) = output_size(p, mw, mh);
                if (nw, nh) != (out_w, out_h) {
                    tracing::info!(
                        from_w = out_w, from_h = out_h, to_w = nw, to_h = nh,
                        mon_w = mw, mon_h = mh,
                        "monitor fallback: resizing encoder output"
                    );
                    total_bytes += switch_encoder_resolution(
                        p, &d3d, nw, nh, &mut venc, &mut writer, &mut vbuf, &mut abuf, &status,
                    )?;
                    out_w = nw;
                    out_h = nh;
                }
            }
        }

        // audio
        loopback.poll(&mut audio_buf)?;
        if let Some(base) = base {
            for mut f in audio_buf.drain(..) {
                f.time_100ns = (f.time_100ns - base).max(0);
                aenc.encode(f, &mut abuf)?;
            }
        }

        // mux whatever is ready, in DTS order (short reorder window)
        total_bytes += drain_to_writer(&mut writer, &mut vbuf, &mut abuf, false)?;
        emit_segment_events(&mut writer, &status);

        if last_status.elapsed() >= Duration::from_millis(500) {
            last_status = Instant::now();
            let _ = status.send(RecorderMsg::Status {
                state: "capturing".into(),
                duration_ms: Some(started.elapsed().as_millis() as i64),
                size_bytes: Some(total_bytes),
            });
            let _ = retention.cleanup(writer.active_segment_id());
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    // stop: flush encoders and final packets
    venc.flush(&mut vbuf)?;
    aenc.flush(&mut abuf)?;
    total_bytes += drain_to_writer(&mut writer, &mut vbuf, &mut abuf, true)?;
    writer.finish()?;
    emit_segment_events(&mut writer, &status);
    let _ = retention.cleanup(None);

    let _ = status.send(RecorderMsg::Status {
        state: "stopped".into(),
        duration_ms: Some(started.elapsed().as_millis() as i64),
        size_bytes: Some(total_bytes),
    });
    Ok(())
}

/// Encoder output size for a given (cropped) source size, honoring the preset's
/// resolution mode. Always even. `AutoFit` caps at the preset and never
/// upscales; `Fixed` always uses the preset width×height.
fn output_size(p: &RecordingPreset, eff_w: u32, eff_h: u32) -> (u32, u32) {
    use rec_core::preset::ResolutionMode;
    let (pw, ph) = (p.video.width as u32, p.video.height as u32);
    match p.video.resolution_mode {
        ResolutionMode::Fixed => (pw.max(2) & !1, ph.max(2) & !1),
        ResolutionMode::AutoFit => (pw.min(eff_w.max(2)) & !1, ph.min(eff_h.max(2)) & !1),
    }
}

/// Flush the current encoder, start a fresh segment, and rebuild the HEVC
/// encoder at `nw`×`nh`. The caller still rebuilds the NV12 converter (it owns
/// the source→output mapping) and updates its `out_w`/`out_h`. Returns the bytes
/// drained into the closing segment.
#[allow(clippy::too_many_arguments)]
fn switch_encoder_resolution(
    p: &RecordingPreset,
    d3d: &D3dDevice,
    nw: u32,
    nh: u32,
    venc: &mut MfHevcEncoder,
    writer: &mut SegmentWriter,
    vbuf: &mut Vec<EncodedPacket>,
    abuf: &mut Vec<EncodedPacket>,
    status: &Sender<RecorderMsg>,
) -> Result<i64> {
    // Flush the old encoder's tail into the current (old-res) segment, then close it.
    venc.flush(vbuf)?;
    let written = drain_to_writer(writer, vbuf, abuf, true)?;
    emit_segment_events(writer, status);

    let vcfg = VideoEncoderConfig {
        width: nw,
        height: nh,
        fps_num: p.video.fps as u32,
        fps_den: 1,
        bitrate_bps: (p.video.bitrate_mbps as u32) * 1_000_000,
        gop: p.gop_frames() as u32,
        cbr: matches!(p.video.rate_control, rec_core::preset::RateControl::Cbr),
    };
    *venc = MfHevcEncoder::new(vcfg, &d3d.device).context("HEVC encoder (resize)")?;

    // New parameter sets + a fresh segment starting at this resolution.
    writer.set_stream_params(venc.codec_private());
    writer.force_rotate();
    Ok(written)
}

/// Merge buffered packets by DTS and write those safely past the reorder window.
fn drain_to_writer(
    writer: &mut SegmentWriter,
    vbuf: &mut Vec<EncodedPacket>,
    abuf: &mut Vec<EncodedPacket>,
    flush_all: bool,
) -> Result<i64> {
    // 500ms reorder window in 90kHz ticks (spec §23).
    const REORDER_90K: i64 = 45_000;
    let mut merged: Vec<EncodedPacket> = Vec::new();
    merged.append(vbuf);
    merged.append(abuf);
    merged.sort_by_key(|p| p.dts_90k);

    if merged.is_empty() {
        return Ok(0);
    }
    let max_dts = merged.iter().map(|p| p.dts_90k).max().unwrap_or(0);
    let cutoff = if flush_all { i64::MAX } else { max_dts - REORDER_90K };

    let mut written = 0i64;
    let mut leftover: Vec<EncodedPacket> = Vec::new();
    for pkt in merged {
        if pkt.dts_90k <= cutoff {
            written += pkt.data.len() as i64;
            writer.write_packet(&pkt)?;
        } else {
            leftover.push(pkt);
        }
    }
    // put unwritten packets back into vbuf (order re-established next pass)
    *vbuf = leftover;
    Ok(written)
}

fn emit_segment_events(writer: &mut SegmentWriter, status: &Sender<RecorderMsg>) {
    for closed in writer.drain_closed() {
        let _ = status.send(RecorderMsg::SegmentClosed {
            path: closed.path.to_string_lossy().into_owned(),
            size_bytes: closed.size_bytes,
        });
    }
}

/// Client-area crop within the captured window frame (excludes title bar/borders).
/// `None` for fullscreen/borderless windows (client fills the frame). Uses the DWM
/// extended frame bounds, which WGC per-window capture aligns to.
fn compute_client_crop(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut ext = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut ext as *mut RECT as *mut core::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
        {
            return None;
        }
        let mut client = RECT::default();
        if GetClientRect(hwnd, &mut client).is_err() {
            return None;
        }
        let cw = client.right - client.left;
        let ch = client.bottom - client.top;
        if cw <= 0 || ch <= 0 {
            return None;
        }
        let mut origin = POINT { x: 0, y: 0 };
        if !ClientToScreen(hwnd, &mut origin).as_bool() {
            return None;
        }
        let (ext_w, ext_h) = (ext.right - ext.left, ext.bottom - ext.top);
        // Fullscreen / borderless: client already fills the frame → no crop.
        if (ext_w - cw).abs() <= 2 && (ext_h - ch).abs() <= 2 {
            return None;
        }
        let left = origin.x - ext.left;
        let top = origin.y - ext.top;
        if left < 0 || top < 0 || left + cw > ext_w || top + ch > ext_h {
            return None; // unexpected geometry; don't risk a bad crop
        }
        Some(RECT { left, top, right: left + cw, bottom: top + ch })
    }
}

/// Resolve the PID owning a window (spec §10 audio target start).
pub fn window_pid(hwnd: i64) -> u32 {
    let mut pid = 0u32;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
            HWND(hwnd as *mut core::ffi::c_void),
            Some(&mut pid),
        );
    }
    pid
}
