//! Process-wide tracing setup.

use std::path::Path;
use std::sync::Once;
use tracing_appender::non_blocking::WorkerGuard;

static INIT: Once = Once::new();

/// Initialise tracing once per process (console only). Honours `RUST_LOG`,
/// defaulting to `info`. Safe to call from every binary's `main`.
pub fn init() {
    INIT.call_once(|| {
        use tracing_subscriber::{fmt, EnvFilter};
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        fmt().with_env_filter(filter).with_target(false).init();
    });
}

/// Initialise tracing to BOTH the console and a daily rolling file under
/// `log_dir` (`recorder.log.YYYY-MM-DD`). The returned guard must be kept alive
/// for the duration of the process so buffered logs are flushed.
pub fn init_with_file(log_dir: &Path) -> Option<WorkerGuard> {
    let _ = std::fs::create_dir_all(log_dir);
    let file_appender = tracing_appender::rolling::daily(log_dir, "recorder.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let mut result = None;
    INIT.call_once(|| {
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::{fmt, EnvFilter};
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let console = fmt::layer().with_target(false);
        let file = fmt::layer()
            .with_ansi(false)
            .with_target(false)
            .with_writer(non_blocking);
        tracing_subscriber::registry()
            .with(filter)
            .with(console)
            .with(file)
            .init();
        result = Some(guard);
    });
    result
}
