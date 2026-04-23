# we-forgectl (Rust port — v0.3.0-dev)

Rust port of the Python single-file CLI at `../scripts/we-forgectl`. Goal:
ship a single static binary (no Python interpreter required) for macOS,
Linux, and Windows — same UX as the Python version, ~5MB instead of ~30MB
RAM, ~5ms startup vs ~150ms.

## Status (v0.3.0-dev — initial scaffold)

This is **not yet feature-complete.** Use the Python version
(`../scripts/we-forgectl`) for production. The Rust port progresses one
session at a time, each session shipping one or more working subcommands.

| Subcommand          | Status              | Notes                                       |
| ------------------- | ------------------- | ------------------------------------------- |
| `install` (macOS)   | ✅ usable           | LaunchdManager full impl, atomic plist, KeepAlive=true / scheduled mode |
| `install` (Linux)   | ⏳ stub             | systemd unit gen — port from Python next session |
| `install` (Windows) | ⏳ stub             | Task Scheduler — port from Python next session |
| `uninstall`         | ✅ usable           | safety-guard backup pattern, Mac only      |
| `start/stop/restart`| ✅ usable (macOS)   | launchctl kickstart/stop                    |
| `status`            | ✅ usable           | launchctl print parsing                     |
| `daemon`            | ⏳ stub             | placeholder; no actual loop yet             |
| `run-once`          | ✅ usable           | spawns `bash tick.sh`                       |
| `tui`               | ⏳ stub             | text-only menu; ratatui live UI next        |
| `dashboard`         | ✅ usable           | shells out to dashboard.py                  |
| `notify-test`       | ⏳ stub             | needs daemon::telegram (reqwest) impl       |
| `doctor`            | ✅ usable           | full parity with Python                     |
| `logs`              | ✅ usable           | full parity                                 |
| `ecc-log`           | ✅ usable           | full parity                                 |
| `ecc-trace`         | ✅ usable           | full parity                                 |

## ECC alignment (this crate is a reference impl of these skills)

Embedded in `Cargo.toml` and per-module doc comments:

| ECC marketplace skill          | Where in this crate                  |
| ------------------------------ | ------------------------------------ |
| `autonomous-agent-harness`     | `daemon::run`, `service::launchd::install` (KeepAlive=true) |
| `enterprise-agent-ops`         | All of `service::*`, `cli::lifecycle`, `cli::install`, `cli::uninstall` |
| `safety-guard`                 | `core::atomic_write`, `cli::uninstall` (backup-before-destroy) |
| `dashboard-builder`            | `cli::dashboard` (delegated), `tui` (planned ratatui UI) |
| `messages-ops`                 | `daemon::telegram` (planned)         |
| `continuous-agent-loop`        | `daemon::tick`, scheduled mode       |
| `architecture-decision-records`| `core::ecc::log` (the ECC trace ledger itself) |
| `rust-patterns`                | Whole crate (idiomatic Rust)         |
| `rust-build`                   | Cargo error remediation (rust-build-resolver agent) |

Every CLI subcommand call also writes to `~/.we-forge/ecc-trace.jsonl`, so
the binary itself reports — *at runtime* — which ECC skills it's leveraging.
Run `we-forgectl ecc-trace --group` to see the totals.

## Build

```bash
# Install Rust if you don't have it
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# (or: brew install rust)

# Build release binary
cd rust
cargo build --release

# Binary lands at: target/release/we-forgectl
./target/release/we-forgectl --help
./target/release/we-forgectl status
```

## Install (replaces the Python version on PATH)

```bash
cargo build --release
cp target/release/we-forgectl ~/.local/bin/we-forgectl
we-forgectl install                  # registers launchd / systemd / Task Sched
we-forgectl install --enable-telegram # daemon mode + Telegram (when impl complete)
```

The Rust binary writes the **exact same** `~/.we-forge/config.json` and
`~/.we-forge/ecc-trace.jsonl` formats as the Python version, so you can
swap freely without losing state.

## Roadmap (per session)

- **v0.3.0** (this session) — scaffold + macOS LaunchdManager full impl + ECC trace
- **v0.3.1** — port systemd manager (Linux)
- **v0.3.2** — port Task Scheduler manager (Windows; PowerShell shellouts for now)
- **v0.3.3** — port full daemon_loop with tokio::select! for parallel tick + Telegram
- **v0.3.4** — Telegram client (reqwest, async)
- **v0.4.0** — ratatui live TUI (cokacctl-style menu)
- **v0.5.0** — GitHub Actions cross-compile + Release artifacts (macOS/Linux/Windows)

## Architecture

Flat-module layout to keep the build graph simple:

```
src/
├── main.rs    — clap CLI dispatcher
├── core.rs    — paths, config (~/.we-forge/config.json), ECC ledger,
│                atomic_write, OS detection, ISO-8601 UTC time
├── service.rs — ServiceManager trait + launchd / systemd / taskscheduler
├── cli.rs     — install, uninstall, lifecycle, status, dashboard, doctor,
│                logs, ecc::{log,trace}, notify_test
├── daemon.rs  — async daemon loop + tick subprocess + Telegram (stubs)
└── tui.rs     — ratatui app (stub)
```

Inspired by:
- [cokacctl (Rust)](https://github.com/kstost/cokacctl) — ServiceManager trait pattern, atomic plist writes, KeepAlive=true daemon
- [hermes-gateway (Python)](https://github.com/NousResearch/hermes-agent/blob/main/scripts/hermes-gateway) — single-file CLI dispatch model

## Coexistence with the Python version

The Rust binary and the Python script can coexist on the same machine:

- Both produce/consume `~/.we-forge/config.json` (same schema)
- Both append to `~/.we-forge/ecc-trace.jsonl` (same JSONL format)
- `launchd` plist points at whichever binary is on PATH at install time
- `we-forgectl uninstall` from either version cleans up correctly

Recommended migration: keep the Python version running (it's stable),
build the Rust version, then `cp target/release/we-forgectl ~/.local/bin/`
and `we-forgectl restart`.
