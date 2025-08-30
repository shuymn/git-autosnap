use anyhow::Result;
use std::collections::VecDeque;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::time::{Duration, interval};

pub async fn show_logs(repo_root: &Path, follow: bool, lines: usize) -> Result<()> {
    let log_dir = repo_root.join(".autosnap");

    // Find the most recent log file
    let log_path = find_latest_log_file(&log_dir).await?;

    if log_path.is_none() {
        println!("No log file found. The watcher may not have been started yet.");
        return Ok(());
    }

    let log_path = log_path.unwrap();

    // Show last N lines
    print_last_lines(&log_path, lines).await?;

    if follow {
        // Watch for changes and print new lines
        follow_file(&log_path).await?;
    }

    Ok(())
}

async fn find_latest_log_file(log_dir: &Path) -> Result<Option<PathBuf>> {
    use tokio::fs;

    // Check if log directory exists
    if !log_dir.exists() {
        return Ok(None);
    }

    // Read directory entries
    let mut entries = fs::read_dir(log_dir).await?;
    let mut log_files = Vec::new();

    // Collect all autosnap.log* files
    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if file_name_str.starts_with("autosnap.log") {
            log_files.push(entry.path());
        }
    }

    // Sort by modification time and return the most recent
    if log_files.is_empty() {
        Ok(None)
    } else {
        log_files.sort_by(|a, b| {
            let a_modified = std::fs::metadata(a)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let b_modified = std::fs::metadata(b)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            a_modified.cmp(&b_modified)
        });

        Ok(Some(log_files.last().unwrap().clone()))
    }
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
