//! Recording indicator: a small blinking red dot in the screen's top-right,
//! shown while recording (replaces the WGC yellow border). Windows-only.
//!
//! A normal opaque window made **circular** with `SetWindowRgn` (the eframe/glow
//! backend can't make a transparent viewport, and layered windows rendered blank
//! here). Always-on-top, click-through (`WS_EX_TRANSPARENT` + `HTTRANSPARENT`),
//! and excluded from screen capture (`WDA_EXCLUDEFROMCAPTURE`) so it never appears
//! in recordings. The red brightness pulses ~1Hz for the blink.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateEllipticRgn, CreateSolidBrush, DeleteObject, EndPaint, FillRect, SetWindowRgn,
    PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const SZ: i32 = 32; // window box
const DOT: i32 = 16; // the visible red dot (small), centered in the box

pub struct RecIndicator {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for RecIndicator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

pub fn show() -> RecIndicator {
    let stop = Arc::new(AtomicBool::new(false));
    let st = stop.clone();
    let handle = std::thread::spawn(move || unsafe { run(st) });
    RecIndicator { stop, handle: Some(handle) }
}

static STOP_FLAG: AtomicBool = AtomicBool::new(false);
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

unsafe fn run(stop: Arc<AtomicBool>) {
    STOP_FLAG.store(false, Ordering::Relaxed);
    START.get_or_init(Instant::now);

    let hinst = GetModuleHandleW(None).unwrap();
    let cls = w!("RokuRecDot");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinst.into(),
        lpszClassName: cls,
        ..Default::default()
    };
    RegisterClassW(&wc);

    let sw = GetSystemMetrics(SM_CXSCREEN);
    let (x, y) = (sw - SZ - 14, 14);
    let sz = SZ;
    let Ok(hwnd) = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
        cls,
        PCWSTR::null(),
        WS_POPUP,
        x,
        y,
        sz,
        sz,
        None,
        None,
        hinst,
        None,
    ) else {
        return;
    };

    // Clip the window to a small centred circle → a small dot within the box.
    let off = (sz - DOT) / 2;
    let rgn = CreateEllipticRgn(off, off, off + DOT + 1, off + DOT + 1);
    let _ = SetWindowRgn(hwnd, rgn, true);

    if std::env::var("ROKU_OVERLAY_VISIBLE_TO_CAPTURE").is_err() {
        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    }
    // Semi-transparent (whole-window constant alpha; reliable, unlike colour-key).
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 120, LWA_ALPHA);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(hwnd, 1, 60, None);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        if stop.load(Ordering::Relaxed) {
            STOP_FLAG.store(true, Ordering::Relaxed);
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            // solid red; the region clips it to the small dot, alpha set separately.
            let brush = CreateSolidBrush(COLORREF(0x0028_28E1)); // (225,40,40)
            FillRect(hdc, &rc, brush);
            let _ = DeleteObject(brush);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_TIMER => {
            if STOP_FLAG.load(Ordering::Relaxed) {
                let _ = DestroyWindow(hwnd);
            } else {
                // Blink via whole-window alpha — always semi-transparent (~28%-59%).
                let t = START.get().map(|s| s.elapsed().as_millis()).unwrap_or(0);
                let phase = (t % 1000) as f32 / 1000.0;
                let a = 72.0 + 78.0 * (0.5 + 0.5 * (phase * std::f32::consts::TAU).sin());
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), a as u8, LWA_ALPHA);
            }
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
