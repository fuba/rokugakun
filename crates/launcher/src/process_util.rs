//! Game launching via `ShellExecuteExW` (spec §21). Windows-only.
//!
//! Handles `.exe`, `.lnk`, `shell:AppsFolder\<AUMID>`, and URIs uniformly. The
//! returned process id may be absent (UWP / Game Pass titles re-launch through a
//! broker), in which case the caller falls back to window detection.

use anyhow::Context;
use windows::core::PCWSTR;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::GetProcessId;
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Outcome of a launch attempt.
#[derive(Debug, Clone)]
pub struct Launched {
    /// PID of the spawned process, if the shell handed one back.
    pub process_id: Option<u32>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Launch a game. `command` is the exe/lnk/AppsFolder/URI; `args`/`workdir` are
/// optional.
pub fn launch(command: &str, args: Option<&str>, workdir: Option<&str>) -> anyhow::Result<Launched> {
    // Some shell targets (AppsFolder) require an initialized COM apartment.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let file_w = wide(command);
    let args_w = args.map(wide);
    let dir_w = workdir.map(wide);

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpFile: PCWSTR(file_w.as_ptr()),
        lpParameters: args_w
            .as_ref()
            .map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr())),
        lpDirectory: dir_w
            .as_ref()
            .map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    // SAFETY: all PCWSTRs point into buffers kept alive until after the call.
    unsafe { ShellExecuteExW(&mut info) }
        .with_context(|| format!("ShellExecuteExW failed for {command}"))?;

    let process_id = if info.hProcess.is_invalid() {
        None
    } else {
        let pid = unsafe { GetProcessId(info.hProcess) };
        (pid != 0).then_some(pid)
    };

    Ok(Launched { process_id })
}
