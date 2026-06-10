//! Top-level window enumeration via Win32 (spec §6/§21). Windows-only.
//!
//! Produces [`WindowInfo`] snapshots that the platform-independent
//! [`crate::matcher`] ranks against a saved [`WindowRule`].

use crate::matcher::WindowInfo;
use std::path::Path;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

/// Enumerate visible top-level windows with their owning process info.
pub fn enumerate() -> Vec<WindowInfo> {
    let mut hwnds: Vec<HWND> = Vec::new();
    // SAFETY: callback only pushes into the Vec referenced by lparam for the
    // duration of the call; EnumWindows is synchronous.
    unsafe {
        let _ = EnumWindows(Some(collect_hwnds), LPARAM(&mut hwnds as *mut _ as isize));
    }

    hwnds
        .into_iter()
        .map(build_info)
        .collect()
}

/// PID owning a window (spec §10: audio target resolution starts here).
pub fn window_pid(hwnd: i64) -> u32 {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(HWND(hwnd as *mut core::ffi::c_void), Some(&mut pid));
    }
    pid
}

unsafe extern "system" fn collect_hwnds(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let vec = &mut *(lparam.0 as *mut Vec<HWND>);
    vec.push(hwnd);
    BOOL(1) // continue enumeration
}

fn build_info(hwnd: HWND) -> WindowInfo {
    let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
    let title = window_text(hwnd);
    let class = class_name(hwnd);

    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    let image_path = process_image_path(pid);
    let process_name = image_path.as_deref().and_then(file_name);

    WindowInfo {
        hwnd: hwnd.0 as i64,
        pid,
        title,
        class,
        image_path,
        process_name,
        aumid: None, // packaged-app AUMID resolution deferred (spec §21 fallback)
        monitor_index: None,
        visible,
    }
}

fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

/// Full image path for a pid via the limited-query right (works for most games).
fn process_image_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle: HANDLE =
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
}

fn file_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
}
