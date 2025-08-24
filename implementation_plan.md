# Implementation Plan for `git autosnap logs` Command

## Overview
Implement a `logs` command for git-autosnap to view daemon logs, similar to `docker compose logs`, with support for following log output in real-time.

## Core Design Decisions

1. **Log Storage**: Single file `.autosnap/autosnap.log` with daily rotation
2. **Format**: Human-readable text with timestamp and log level
3. **CLI Options**: Minimal - just `-f/--follow` and `-n/--lines`
4. **Dependencies**: Use existing dependencies (tokio) for following, add only `tracing-appender` for rotation

## Implementation Steps

### 1. Add Log File Writer (`src/logger.rs`)

Create a new module to handle file-based logging with rotation:

```rust
use std::path::Path;
use anyhow::Result;
use tracing_appender::rolling;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init_file_logger(repo_root: &Path, is_daemon: bool) -> Result<()> {
    let log_dir = repo_root.join(".autosnap");
    let file_appender = rolling::daily(log_dir, "autosnap.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    
    let fmt_layer = fmt::layer()
        .with_ansi(false)
        .with_target(false)
        .with_writer(non_blocking);
    
    // Also output to console if not daemon
    if !is_daemon {
        // Dual output: file + console
        // Use fmt::layer() with stdout for console output
    }
    
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(fmt_layer)
        .init();
    
    Ok(())
}
```

### 2. Update CLI (`src/cli.rs`)

Add the new `Logs` command variant:

```rust
/// View watcher logs
Logs {
    /// Follow log output (like tail -f)
    #[arg(short, long)]
    follow: bool,
    
    /// Number of lines to show (default: 100)
    #[arg(short = 'n', long, default_value = "100")]
    lines: usize,
}
```

### 3. Implement Log Reader (`src/logs.rs`)

Create the log reading and following functionality using tokio's async capabilities:

```rust
use std::path::Path;
use anyhow::Result;
use tokio::time::{interval, Duration};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use std::io::SeekFrom;
use std::collections::VecDeque;

pub async fn show_logs(repo_root: &Path, follow: bool, lines: usize) -> Result<()> {
    let log_path = repo_root.join(".autosnap/autosnap.log");
    
    if !log_path.exists() {
        println!("No log file found. The watcher may not have been started yet.");
        return Ok(());
    }
    
    // Show last N lines
    print_last_lines(&log_path, lines).await?;
    
    if follow {
        // Watch for changes and print new lines
        follow_file(&log_path).await?;
    }
    
    Ok(())
}

async fn print_last_lines(path: &Path, n: usize) -> Result<()> {
    let file = File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines_buffer = VecDeque::with_capacity(n);
    let mut lines = reader.lines();
    
    // Read all lines into a circular buffer
    while let Some(line) = lines.next_line().await? {
        if lines_buffer.len() == n {
            lines_buffer.pop_front();
        }
        lines_buffer.push_back(line);
    }
    
    // Print the last N lines
    for line in lines_buffer {
        println!("{}", line);
    }
    
    Ok(())
}

async fn follow_file(path: &Path) -> Result<()> {
    let mut file = File::open(path).await?;
    let mut last_size = file.metadata().await?.len();
    
    // Seek to end to start following from current position
    file.seek(SeekFrom::End(0)).await?;
    
    let mut interval = interval(Duration::from_millis(250)); // Poll 4 times per second
    
    loop {
        interval.tick().await;
        
        let metadata = tokio::fs::metadata(path).await?;
        let current_size = metadata.len();
        
        if current_size > last_size {
            // New content detected, read it
            let mut reader = BufReader::new(&mut file);
            let mut line = String::new();
            
            while reader.read_line(&mut line).await? > 0 {
                print!("{}", line);
                line.clear();
            }
            
            last_size = current_size;
        } else if current_size < last_size {
            // File was truncated (log rotation), reopen from beginning
            file = File::open(path).await?;
            last_size = 0;
        }
    }
}
```

### 4. Update Watcher Initialization

Modify `src/watcher.rs::start_foreground()` to initialize file logging:

```rust
pub fn start_foreground(repo_root: &Path, cfg: &AutosnapConfig) -> Result<()> {
    // Initialize file logger
    crate::logger::init_file_logger(repo_root, false)?;
    
    // ... rest of existing code
}
```

### 5. Update Daemon Mode

Modify `src/daemon.rs` to use file-only logging when daemonized:

```rust
pub fn daemonize(repo_root: &Path) -> Result<()> {
    // ... existing daemonization code ...
    
    // Initialize file-only logger for daemon
    crate::logger::init_file_logger(repo_root, true)?;
    
    // ... rest of existing code
}
```

### 6. Update Main Entry Point

Add handling for the `Logs` command in `src/lib.rs`:

```rust
Commands::Logs { follow, lines } => {
    let repo_root = gitlayer::find_repo_root()?;
    
    // Use tokio runtime for async file operations
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(crate::logs::show_logs(&repo_root, follow, lines))?;
}
```

## Dependencies

Add the required dependency using cargo:

```bash
cargo add tracing-appender
```

## Testing Strategy

1. **Unit tests**: Test log parsing and line counting logic
2. **Integration tests**: Test log following with simulated file writes
3. **Container tests**: Test full daemon logging and reading flow

## Benefits of This Approach

1. **Minimal dependencies**: Only adds `tracing-appender` for rotation
2. **Simple and reliable**: Polling is straightforward and works everywhere  
3. **Efficient**: 250ms polling interval is responsive without being wasteful
4. **Handles rotation**: Detects file truncation and reopens
5. **Already async**: Uses tokio which we already have
6. **Familiar UX**: Works like `docker logs` or `tail -f`

## Coding Standards Reference

This implementation should follow the project's coding standards:
- See [docs/style.md](docs/style.md) for Rust coding standards
- See [docs/testing.md](docs/testing.md) for testing guidelines

## Future Enhancements

- Add `--since` option to filter by timestamp
- Support JSON output format for structured logging
- Add log level filtering
- Implement log compression for old files
