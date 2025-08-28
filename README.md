# git-autosnap

Record working tree snapshots into a local bare repo (.autosnap) while you edit, without touching your main Git history. Restore, diff, or explore snapshots anytime.

## Features

- Debounced file watching that respects .gitignore (via watchexec)
- Snapshots stored as commits in `.autosnap` with messages like `AUTOSNAP[branch] ISO8601 [optional]`
- Safe restore (overlay) or full restore, with `--dry-run` preview and `--force` override
- Rich diff views (unified/stat/name-only/name-status) between snapshots or vs working tree
- Interactive selection (`-i`) using skim for shell/restore/diff
- Single-instance guard with PID lock at `.autosnap/autosnap.pid`
- Graceful shutdown and hot-reload via Unix signals
- Rolling file logs at `.autosnap/autosnap.log*`

## Quick Start

```bash
# Inside an existing Git repository
git autosnap init             # or: git-autosnap init
git autosnap start            # run foreground watcher

# …edit files… snapshots are created automatically

git autosnap once "WIP note"  # take one snapshot immediately (optional)
git autosnap diff             # view diff vs latest snapshot
git autosnap restore -i       # interactively restore a snapshot

git autosnap stop             # stop background daemon (if used)
git autosnap uninstall        # remove .autosnap after stopping
```

You can invoke as `git autosnap …` (Git external command) or `git-autosnap …` (direct).

## CLI

```text
git-autosnap [FLAGS] <SUBCOMMAND>

Subcommands
  init                         Initialize .autosnap in the current repo
  start [--daemon]             Launch watcher (foreground or daemon)
  stop                         Stop background watcher
  status                       Exit 0 if running, non‑zero otherwise
  once [MESSAGE]               Take a single snapshot and print its short hash
  gc [--prune --days N]        Compress objects; optionally prune old snapshots
  uninstall                    Stop and remove .autosnap directory
  shell [-i] [COMMIT]          Extract a snapshot and open a subshell to explore
  restore [-i --force --dry-run --full] [COMMIT] [PATH...]
                               Restore all or specific paths from a snapshot
  diff [-i | --stat | --name-only | --name-status] [COMMIT1] [COMMIT2] [PATH...]
                               Show changes between snapshots or vs working tree
  logs [-f -n LINES]           Tail watcher log file (defaults: follow=false, n=100)
```

## How It Works

- A hidden bare repository lives at `.autosnap/` inside your repo. Snapshots are commits there.
- The watcher batches rapid changes using a debounce window and skips redundant commits when the tree is unchanged.
- `.autosnap` is automatically added to `.git/info/exclude` so it never appears in `git status`.

## Configuration (git config)

Use standard Git config scopes (local → global → system):

```bash
# Debounce window in milliseconds (default: 1000)
git config autosnap.debounce-ms 1000

# GC prune retention in days (default: 60)
git config autosnap.gc.prune-days 60
```

## Signals & Process Control

- PID lock file: `.autosnap/autosnap.pid` (single instance)
- Signals handled by the watcher:
  - SIGTERM/SIGINT: take a final snapshot, then exit
  - SIGUSR1: force an immediate snapshot
  - SIGUSR2: prepare snapshot and exec the updated binary once it changes on disk

Example: `kill -USR1 $(cat ./.autosnap/autosnap.pid)`

## Logging

- Logs are written to `.autosnap/autosnap.log*` (daily rotation)
- View logs: `git autosnap logs -n 200 --follow`

## Install / Build (Taskfile first)

```bash
# Build (debug / release)
task build
task build-release

# Install into git exec-path with hot‑reload if daemon is running
task install

# Run with args (defaults to --help)
task run -- ARGS="--help"
```

Cargo alternatives (if you prefer raw cargo): `cargo build`, `cargo build --release`, `cargo run -- --help`.

## Development

```bash
# Format, lint, and watch
task fmt
task clippy
task lint
task watch

# Clean artifacts
task clean

# Docs
task docs            # or: task docs-all
```

Verbose logging: pass `-v` / `-vv` or set `RUST_LOG=git_autosnap=debug`.

## Testing

```bash
task test           # runs unit + container tests
task test-unit      # unit tests only (nextest)
task test-container # container tests (requires Docker)
```

- Integration tests favor containers for isolation (see `docs/testing.md`).
- Tests must not touch host Git state or global configs.

## License

MIT © 2025 Shu YAMANI (see `LICENSE`).
