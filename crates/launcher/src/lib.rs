//! Launcher library.
//!
//! - `matcher`        — platform-independent window-rule matching (unit-tested)
//! - `window_detect`  — EnumWindows-based enumeration (Windows only)
//! - `process_util`   — ShellExecuteEx launch + process queries (Windows only)

pub mod matcher;
pub mod server;

#[cfg(windows)]
pub mod overlay;

#[cfg(windows)]
pub mod process_util;
#[cfg(windows)]
pub mod session;
#[cfg(windows)]
pub mod window_detect;
