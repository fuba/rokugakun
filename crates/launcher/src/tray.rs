//! System-tray (notification-area) icon. Windows-only.
//!
//! Runs a tiny Win32 message loop on its own thread (same shape as `overlay`):
//! it creates an invisible top-level window, registers a `Shell_NotifyIcon`
//! icon, and pumps messages. Left-click / double-click restores the main
//! window; right-click shows an Open / Quit menu. All UI actions reach the
//! egui app through a shared [`egui::Context`] (viewport commands + repaint),
//! so the tray thread never has to touch the eframe window directly.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Private callback message the shell posts for our icon (WM_APP range).
const WM_TRAY: u32 = WM_APP + 1;
const TRAY_UID: u32 = 1;
const IDM_OPEN: usize = 1;
const IDM_QUIT: usize = 2;

/// Title of the eframe main window (must match `run_native`), used to find its
/// HWND so we can show/hide it at the OS level.
const MAIN_TITLE: PCWSTR = w!("rokugakun — game auto-recorder");

/// The egui context, shared with the tray thread for cross-thread commands.
static CTX: OnceLock<egui::Context> = OnceLock::new();
/// Cached HWND of the eframe main window (0 until first resolved).
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
/// Set when the user picks "Quit" from the tray, so the app's close handler
/// knows to actually exit instead of hiding back to the tray.
static FORCE_QUIT: AtomicBool = AtomicBool::new(false);

/// True if the user asked to quit from the tray menu.
pub fn quit_requested() -> bool {
    FORCE_QUIT.load(Ordering::Relaxed)
}

/// Owns the tray thread; removing the icon on drop.
pub struct Tray {
    hwnd: Arc<AtomicIsize>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for Tray {
    fn drop(&mut self) {
        let h = self.hwnd.load(Ordering::Relaxed);
        if h != 0 {
            unsafe {
                let _ = PostMessageW(HWND(h as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(j) = self.handle.take() {
            let _ = j.join();
        }
    }
}

/// Start the tray icon. `ctx` is the running egui context (commands sent to it
/// reach the app on its next frame).
pub fn start(ctx: egui::Context) -> Tray {
    let _ = CTX.set(ctx);
    let hwnd_slot = Arc::new(AtomicIsize::new(0));
    let slot = hwnd_slot.clone();
    let handle = std::thread::spawn(move || unsafe { run(slot) });
    Tray { hwnd: hwnd_slot, handle: Some(handle) }
}

/// Record the eframe main-window HWND (called once from the app's first frame),
/// so show/quit don't have to look it up.
pub fn set_main_hwnd(hwnd: isize) {
    MAIN_HWND.store(hwnd, Ordering::Relaxed);
}

/// Resolve the eframe main window: the cached handle, else by title.
fn main_hwnd() -> HWND {
    let cached = MAIN_HWND.load(Ordering::Relaxed);
    if cached != 0 {
        return HWND(cached as *mut _);
    }
    unsafe { FindWindowW(None, MAIN_TITLE).unwrap_or_default() }
}

/// Ask the app to restore + focus its window.
///
/// A window hidden at the OS level runs no egui frames, so viewport commands
/// alone can't bring it back (no frame ever processes them). We first poke the
/// real Win32 window visible to unblock the frame loop, *then* send the
/// commands to sync winit's state and take focus.
fn request_show() {
    let hwnd = main_hwnd();
    if !hwnd.is_invalid() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
    if let Some(ctx) = CTX.get() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

/// Ask the app to quit for real (close, not hide-to-tray).
fn request_quit() {
    FORCE_QUIT.store(true, Ordering::Relaxed);
    let hwnd = main_hwnd();
    if !hwnd.is_invalid() {
        // Show first so the close request is actually processed if we were
        // sitting hidden in the tray, then ask the window to close.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
    if let Some(ctx) = CTX.get() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        ctx.request_repaint();
    }
}

unsafe fn run(slot: Arc<AtomicIsize>) {
    let hinst = GetModuleHandleW(None).unwrap();
    let cls = w!("RokuTray");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinst.into(),
        lpszClassName: cls,
        ..Default::default()
    };
    RegisterClassW(&wc);

    // An invisible (never shown) top-level window — message-only windows can't
    // become foreground, which a popup menu needs to dismiss correctly.
    let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        cls,
        w!("rokugakun"),
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        None,
        None,
        hinst,
        None,
    ) else {
        return;
    };
    slot.store(hwnd.0 as isize, Ordering::Relaxed);

    let icon = load_icon();
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    for (i, c) in "rokugakun".encode_utf16().enumerate() {
        nid.szTip[i] = c;
    }
    let _ = Shell_NotifyIconW(NIM_ADD, &nid);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    if !icon.is_invalid() {
        let _ = DestroyIcon(icon);
    }
}

/// Load the exe-embedded app icon (winresource emits it as resource id 1),
/// at notification-area size. Falls back to the generic application icon.
unsafe fn load_icon() -> HICON {
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    let cx = GetSystemMetrics(SM_CXSMICON);
    let cy = GetSystemMetrics(SM_CYSMICON);
    if let Ok(h) = LoadImageW(hinst, PCWSTR(1 as *const u16), IMAGE_ICON, cx, cy, LR_DEFAULTCOLOR) {
        return HICON(h.0);
    }
    LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
}

unsafe fn show_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else { return };
    let _ = AppendMenuW(menu, MF_STRING, IDM_OPEN, w!("Open rokugakun"));
    let _ = AppendMenuW(menu, MF_STRING, IDM_QUIT, w!("Quit"));
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    // Required so the menu dismisses when the user clicks elsewhere.
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            // Low word of lParam is the mouse message (default icon version).
            match (lp.0 as u32) & 0xFFFF {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => request_show(),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wp.0 & 0xFFFF {
                IDM_OPEN => request_show(),
                IDM_QUIT => request_quit(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
