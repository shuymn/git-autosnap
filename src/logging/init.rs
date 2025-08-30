use std::{path::Path, sync::Mutex};

use anyhow::{Context, Result};

// Global guard to keep the file appender alive
static FILE_APPENDER_GUARD: Mutex<Option<tracing_appender::non_blocking::WorkerGuard>> =
    Mutex::new(None);

/// Flush and close the log file appender.
/// This should be called before exec to ensure all buffered logs are written.
pub fn flush_logs() {
    // Taking the guard will drop it, which flushes pending logs
    if let Ok(mut guard_holder) = FILE_APPENDER_GUARD.lock()
        && let Some(guard) = guard_holder.take()
    {
        // Explicitly drop to ensure flush happens before continuing
        drop(guard);
        // Give the filesystem a moment to complete the write
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Initialize tracing. RUST_LOG (if set) takes precedence.
/// Otherwise, -v/-vv map to "debug"/"trace".
pub fn init_tracing(verbosity: u8) -> Result<()> {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let base = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| base.to_string());

    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_new(filter).context("invalid RUST_LOG / filter")?;

    // Allow re-init to be a no-op in tests
    let _ = tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .try_init();

    Ok(())
}

/// Initialize tracing with file logging. RUST_LOG (if set) takes precedence.
/// Otherwise, -v/-vv map to "debug"/"trace".
pub fn init_tracing_with_file(repo_root: &Path, verbosity: u8, is_daemon: bool) -> Result<()> {
    use tracing_appender::rolling;
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let base = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| base.to_string());
    let filter_layer = EnvFilter::try_new(filter).context("invalid RUST_LOG / filter")?;

    let log_dir = repo_root.join(".autosnap");
    let file_appender = rolling::daily(log_dir, "autosnap.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Store the guard globally to keep it alive for the program duration
    if let Ok(mut guard_holder) = FILE_APPENDER_GUARD.lock() {
        *guard_holder = Some(guard);
    }

    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_target(false)
        .with_writer(non_blocking);

    if is_daemon {
        // File-only logging for daemon mode
        let _ = tracing_subscriber::registry()
            .with(filter_layer)
            .with(file_layer)
            .try_init();
    } else {
        // Dual logging (file + console) for foreground mode
        let console_layer = fmt::layer().with_target(false);

        let _ = tracing_subscriber::registry()
            .with(filter_layer)
            .with(file_layer)
            .with(console_layer)
            .try_init();
    }

    Ok(())
}
