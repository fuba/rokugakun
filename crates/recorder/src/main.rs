//! Recorder process entry point.
//!
//! `recorder demo <out_dir> [seconds]` runs the end-to-end encode+save demo.
//! The full IPC-driven pipeline (state machine + capture/encode/mux threads) is
//! wired in Step 7.

fn main() -> anyhow::Result<()> {
    rec_core::logging::init();
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        #[cfg(windows)]
        Some("demo") => {
            let out_dir = args
                .get(2)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("rokugakun_demo"));
            let seconds: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
            tracing::info!(?out_dir, seconds, "running encode+save demo");
            let path = recorder::demo::run(&out_dir, seconds)?;
            println!("wrote {}", path.display());
        }
        #[cfg(windows)]
        Some("wintest") => {
            let title = args.get(2).cloned().unwrap_or_default();
            let secs: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
            let n = recorder::demo::window_capture_test(&title, secs)?;
            println!("per-window frames captured in {secs}s: {n}");
        }
        #[cfg(windows)]
        Some("live-demo") => {
            let out_dir = args
                .get(2)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("rokugakun_live"));
            let seconds: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
            tracing::info!(?out_dir, seconds, "running live capture demo (ffplay source)");
            let path = recorder::demo::run_live(&out_dir, seconds)?;
            println!("wrote {}", path.display());
        }
        _ => {
            tracing::info!("recorder: pass `demo <out_dir> [seconds]` to run the encode demo");
        }
    }
    Ok(())
}
