//! Run-at-startup, via the per-user `Run` registry key. Windows-only.
//!
//! Writing `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\rokugakun` makes
//! Windows launch the app at sign-in. We point it at the current exe with a
//! `--tray` flag so it boots straight into the notification area (see `tray`).

use anyhow::{bail, Result};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, REG_VALUE_TYPE,
};

/// Sub-key under HKCU holding per-user startup commands.
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Value name we own under that key.
const VALUE_NAME: &str = "rokugakun";

/// `"<exe>" --tray` — launch into the tray at sign-in.
fn startup_command() -> String {
    let exe = std::env::current_exe().unwrap_or_default();
    format!("\"{}\" --tray", exe.display())
}

/// True if we currently have a startup entry registered.
pub fn is_enabled() -> bool {
    current_value().is_some()
}

/// Add or remove the startup entry.
pub fn set_enabled(on: bool) -> Result<()> {
    if on {
        enable()
    } else {
        disable()
    }
}

fn enable() -> Result<()> {
    unsafe {
        let mut hkey = HKEY::default();
        let rc = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_SUBKEY),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        );
        if rc != ERROR_SUCCESS {
            bail!("cannot open Run key: {rc:?}");
        }
        let name = HSTRING::from(VALUE_NAME);
        let wide: Vec<u16> = startup_command().encode_utf16().chain([0]).collect();
        let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
        let rc = RegSetValueExW(hkey, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(bytes));
        let _ = RegCloseKey(hkey);
        if rc != ERROR_SUCCESS {
            bail!("cannot write startup value: {rc:?}");
        }
    }
    Ok(())
}

fn disable() -> Result<()> {
    unsafe {
        let mut hkey = HKEY::default();
        let rc = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_SUBKEY),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        );
        if rc != ERROR_SUCCESS {
            return Ok(()); // key missing → nothing to remove
        }
        let name = HSTRING::from(VALUE_NAME);
        let _ = RegDeleteValueW(hkey, PCWSTR(name.as_ptr()));
        let _ = RegCloseKey(hkey);
    }
    Ok(())
}

/// Read the current startup command string, if any.
fn current_value() -> Option<String> {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_SUBKEY),
            0,
            KEY_QUERY_VALUE,
            &mut hkey,
        ) != ERROR_SUCCESS
        {
            return None;
        }
        let name = HSTRING::from(VALUE_NAME);
        let mut ty = REG_VALUE_TYPE::default();
        let mut len: u32 = 0;
        // First call sizes the buffer.
        let rc = RegQueryValueExW(
            hkey,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty),
            None,
            Some(&mut len),
        );
        if rc != ERROR_SUCCESS || len == 0 {
            let _ = RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        let rc = RegQueryValueExW(
            hkey,
            PCWSTR(name.as_ptr()),
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut len),
        );
        let _ = RegCloseKey(hkey);
        if rc != ERROR_SUCCESS {
            return None;
        }
        let u16s = std::slice::from_raw_parts(buf.as_ptr() as *const u16, len as usize / 2);
        let s = String::from_utf16_lossy(u16s);
        Some(s.trim_end_matches('\0').to_string())
    }
}
