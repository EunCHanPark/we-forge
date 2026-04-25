# we-forge

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/EunCHanPark/we-forge)](https://github.com/EunCHanPark/we-forge/releases)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-blue)
![Rust](https://img.shields.io/badge/rust-stable-orange)
![Python](https://img.shields.io/badge/python-3.8%2B-blue)

> 24/7 background pattern-learning + ECC-marketplace orchestration layer for
> Claude Code. Watches what you actually do, dedupes against the
> [everything-claude-code](https://github.com/affaan-m/everything-claude-code)
> marketplace (485+ skills), and either **recommends an existing skill** or
> **synthesizes a new one** — quietly, on a midnight-aligned schedule.

---

## Why

You install [Claude Code](https://docs.claude.com) plus the
[ECC marketplace](https://github.com/affaan-m/everything-claude-code) and find:

1. You re-derive the same workflow patterns every week.
2. You don't know which marketplace skills already cover what you're doing.
3. The official `continuous-learning-v2` instinct system isn't enough on its
   own to *cross-reference* your activity against the marketplace.

we-forge is the bridge layer:

```
Your Claude Code session
        │
        │ Stop / SubagentStop hook  +  bash history sweep  +  transcript replay
        ▼
~/.claude/learning/data/events.jsonl
        │
        │ aligned tick (default 00:00 / 12:00 local time)
        ▼
patterns.jsonl  (≥3 occurrences across ≥3 distinct sessions)
        │
        ▼
pattern-detector ── dedupes against ──► ECC marketplace (485 skills)
        │                                ECC homunculus instincts + evolved
        │                                we-forge learned skills
        ▼
we-forge agent v2 (11-step workflow)
        ├─ shell primitive?         → DROP (zero-spend, no API call)
        ├─ exists in marketplace?   → ECC_MATCH (recommend, no synthesis)
        ├─ self-reference noise?    → DROP (observer effect filter)
        └─ truly novel?             → synthesize → audit → ~/.claude/skills/learned/
                                              │
                                              └─► ledger.jsonl + Telegram alert
```

Every decision is recorded in `~/.claude/learning/data/ledger.jsonl` for full
audit trail. Every leveraged ECC skill is recorded in
`~/.we-forge/ecc-trace.jsonl` to prove ROI.

---

## Quick Start — one line, zero post-install steps

### macOS / Linux (latest release binary)

```bash
ARCH=$(uname -m); OS=$(uname | tr A-Z a-z)
TRIPLE="${ARCH}-$([ "$OS" = darwin ] && echo apple-darwin || echo unknown-linux-gnu)"
curl -fsSL https://github.com/EunCHanPark/we-forge/releases/latest/download/we-forgectl-${TRIPLE}.tar.gz | tar xz
sudo mv we-forgectl-* /usr/local/bin/we-forgectl
we-forgectl install
```

### Windows (PowerShell)

```powershell
iwr -useb https://raw.githubusercontent.com/EunCHanPark/we-forge/main/install.ps1 | iex
```

This single line:

1. Downloads the latest `we-forgectl.exe` to `$env:USERPROFILE\.local\bin`
2. Adds that directory to your **user PATH** (persists across sessions)
3. Runs `we-forgectl install` — registers Windows Task Scheduler service
4. Future PowerShell windows recognize `we-forgectl` immediately

Optional flags (after `iex`):

```powershell
# Pin to a specific version
iwr -useb https://raw.githubusercontent.com/EunCHanPark/we-forge/main/install.ps1 -OutFile install.ps1
.\install.ps1 -Version v0.4.0

# Custom install location
.\install.ps1 -InstallDir "C:\tools\we-forge"

# Skip Task Scheduler registration
.\install.ps1 -NoServiceInstall

# Enable Telegram bot during install
.\install.ps1 -EnableTelegram
```

> **WSL2 fallback** (legacy): if you prefer the WSL-based install, see
> [WSL-SETUP.md](WSL-SETUP.md). The previous `install.ps1` is preserved
> as `install.ps1.wsl-fallback.bak` after running this installer.

### From a clone — Python (no Rust toolchain)

```bash
git clone https://github.com/EunCHanPark/we-forge.git
cd we-forge
./install.sh                   # auto-registers launchd / systemd / Task Scheduler
./verify.sh                    # confirm
```

### One-line uninstall (with safety backup)

```bash
we-forgectl uninstall          # stops service, backs up config + data
we-forgectl uninstall --deep   # also moves ~/.we-forge and learning/data
```

Backups go to `~/.we-forge/backup/<ISO-timestamp>/` — nothing is ever
deleted, only moved. Restore by `mv`-ing back.

---

## Tick cadence — `set-interval`

Default: **12 hours, aligned to local 00:00** (fires at 00:00 and 12:00).
Both **learning** and **Telegram notification** use the same cadence.

```bash
we-forgectl set-interval 720    # default — 12h (00:00, 12:00)
we-forgectl set-interval 60     # hourly (00:00, 01:00, ..., 23:00)
we-forgectl set-interval 30     # 30 min  (00:00, 00:30, ..., 23:30)
we-forgectl set-interval 1440   # once a day at 00:00
we-forgectl status              # see current cadence + next tick time
```

All slots are aligned to **local 00:00 (midnight)** in your system timezone.
Hot-reloaded by the daemon — no restart required.

---

## Service control — `we-forgectl`

Single CLI for the whole lifecycle:

```bash
we-forgectl status                    # service state + interval + next tick + active sessions
we-forgectl set-interval <minutes>    # change unified cadence (1-1440)
we-forgectl start | stop | restart    # lifecycle
we-forgectl sessions [--window N]     # list active Claude Code sessions (last N minutes)
we-forgectl ping [label]              # register current session with heartbeat (manual fallback)
we-forgectl tui                       # ratatui-powered control TUI
we-forgectl dashboard                 # open http://127.0.0.1:8765 dashboard
we-forgectl logs                      # tail recent ticks
we-forgectl doctor                    # diagnose dependencies
we-forgectl ecc-trace --group         # show ECC marketplace skill usage
we-forgectl ecc-log <skill> <reason>  # record a manual ECC skill leverage
we-forgectl notify-test               # send Telegram ping (if enabled)
we-forgectl install --enable-telegram # opt-in Telegram bot (daemon mode)
we-forgectl uninstall                 # safety-backup → remove
```

---

## Use inside Claude Code

```
/ping-forge       # register this session with we-forge (manual session detection)
/skill-report     # 6-section report: telemetry, top patterns, ECC matches, learned, decisions
/dashboard        # web dashboard at http://127.0.0.1:8765
/dashboard tui    # rich-powered terminal UI (pip install rich)
/dashboard once   # one-shot stdout snapshot, no deps
/watch-and-learn  # manually trigger the synthesize-and-audit loop
```

---

## Session detection — automatic + manual

we-forge automatically detects active Claude Code sessions via transcript file 
timestamps. Two detection modes:

| Mode | Trigger | Method | Window | Command |
|------|---------|--------|--------|---------|
| **Automatic** | On transcript write | Scan `~/.claude/projects/*/` for `.jsonl` mtime | 60 min | `we-forgectl sessions` |
| **Manual** | Session idle / not detected | Heartbeat ping from inside session | 60 min | `! we-forgectl ping [label]` or `/ping-forge` |

**Use manual ping when:**
- Session is idle (no transcript writes)
- Transcript file path changed (new project directory)
- Session exists but transcript mtime is stale
- You want explicit confirmation of attachment

**Heartbeat files** live at `~/.we-forge/heartbeats/<pid>.json` and expire 
automatically after the window (default 60 min). Multiple PCs can ping 
independently — each registers its own sessions.

```bash
# List all active sessions in last 60 minutes (combines both modes)
we-forgectl sessions

# List active sessions in last 2 hours
we-forgectl sessions --window 120

# Manually register this session from inside Claude Code
! we-forgectl ping my-feature-branch
```

The `/ping-forge` slash command is a convenience wrapper for `! we-forgectl ping`.

---

## Telegram bot (optional)

Opt in for Telegram alerts on PASS/ECC_MATCH events. Switches we-forge to
**daemon mode** (long-running; `KeepAlive=true` on macOS, `Restart=always`
on Linux) and enables a remote-control bot.

```bash
# 1. Get a bot token from @BotFather, then your chat id from @userinfobot
export WE_FORGE_TELEGRAM_TOKEN=...
export WE_FORGE_TELEGRAM_CHAT_ID=...

# 2. Install in daemon mode
we-forgectl install --enable-telegram

# 3. Verify
we-forgectl notify-test
```

### Bot commands (Korean responses)

| Command | Action |
|---------|--------|
| `/status`            | 서비스 가동 상태 |
| `/skill_report`      | 학습 KPI 요약 (events / patterns / queue / ledger / TOP 5) |
| `/last_tick`         | 최근 tick 로그 마지막 15줄 |
| `/ecc_trace`         | ECC 마켓플레이스 스킬 사용 통계 |
| `/dashboard`         | 웹 대시보드 접속 안내 |
| `/interval`          | 학습+알림 주기 조회 (현재 cadence + 다음 발화 시각) |
| `/set_interval <분>` | 주기 변경 (예: `/set_interval 30`) |
| `/help`              | 명령어 안내 |

Disable without uninstalling: `we-forgectl install` (re-runs without
`--enable-telegram` reverts to scheduled mode).

---

## Agent v2 — verdict vocabulary

Every queued pattern exits with **exactly one** of five verdicts. Every
verdict is recorded in `~/.claude/learning/data/ledger.jsonl`:

| Verdict       | Meaning                                                       | Sub-agents | Queue action |
|---------------|---------------------------------------------------------------|------------|--------------|
| `PASS`        | Auditor approved, skill installed                             | synthesizer + auditor | remove |
| `REVISE`      | Auditor asked for rewrite                                     | synthesizer + auditor | rewrite (`revise_count += 1`) |
| `REJECT`      | Auditor rejected; pattern poisoned to never re-queue          | synthesizer + auditor | remove |
| `ECC_MATCH`   | Already covered by ECC marketplace skill                      | none (zero-spend)     | remove (no poison) |
| `DROP`        | Shell primitive / single-tool baseline / self-reference noise | none (zero-spend)     | remove (regex blocklist) |

**Zero-spend short-circuits** (DROP + ECC_MATCH) avoid sub-agent dispatch
for ~96% of queue entries in normal use, keeping API costs minimal.

---

## What gets installed

```
~/.claude/
├── agents/                  monitor-sentinel · pattern-detector · skill-synthesizer · quality-auditor · we-forge
├── commands/                /skill-report · /watch-and-learn · /dashboard · /ask-codex · /ask-gemini
├── hooks/stop-telemetry.sh  Stop / SubagentStop hook (always exits 0)
├── learning/
│   ├── tick.sh              entry point with --dangerously-skip-permissions
│   ├── normalize.py         canonicalization + promotion rule
│   ├── redact.sh            secret filter (--self-test)
│   └── data/                events / patterns / queue / ledger / state
├── skills/learned/          synthesized skills (auditor-passed)
└── agent-memory/we-forge/
    ├── MEMORY.md            persistent agent memory across ticks
    └── staging/             skill drafts when canonical path is blocked

~/.we-forge/
├── config.json              {mode, interval_minutes, telegram_*, installed_at}
├── ecc-trace.jsonl          every ECC marketplace skill leverage (ROI proof)
│                             schema: {ts: ISO8601 UTC, skill: str, reason: str, invoker: "cli"|"agent"}
├── heartbeats/              manual session registrations (PID-keyed heartbeat files)
│   └── <pid>.json           {ts, epoch, cwd, pid, label} — expires after window
├── daemon.pid               PID file
├── last_telegram_sent_at    throttle state (single ISO-8601 line)
└── install-pending.sh       fallback installer for staged skills
```

Plus the merged Stop-hook entry in `~/.claude/settings.json` (existing
entries preserved, original backed up to `settings.json.bak.<ISO>`).

---

## Architecture highlights

- **Two-tier learning architecture.** we-forge runs in parallel with ECC's
  own `continuous-learning-v2` (homunculus). They share no storage but
  pattern-detector dedupes against ECC's outputs to avoid duplication.
- **Aligned tick scheduling.** All ticks fire at slots aligned to local
  00:00 (e.g. 30-min interval → 48 slots/day at HH:00 and HH:30). Cleaner
  than "every N seconds since last tick" for predictable ops.
- **Bash-first hot path.** `tick.sh` only spends API credits when the
  promotion queue is non-empty.
- **Secrets dropped, not masked.** `redact.sh` and `normalize.py` share a
  regex + Shannon-entropy filter.
- **Primitive auto-blocklist.** 14+ regex patterns auto-DROP shell
  primitives (`bash-grep-*`, `bash-cat-*`, `bash-find-*`, etc.) without
  sub-agent dispatch, preserving API spend for novel patterns.
- **Self-reference filter.** Patterns whose samples reference
  `~/.claude/learning/` are auto-DROPped (the observer effect — agent
  inspecting its own data shouldn't spawn skills).
- **ECC alignment disclosure.** Every tick output starts with the ECC
  marketplace skills shaping its behavior, recorded to ECC trace.
- **Single-instance lock.** `tick.sh` uses portable `mkdir`-based locking;
  cron/launchd/systemd double-fires are safe no-ops.
- **Atomic writes only.** Queue updates and ledger appends are crash-safe
  (`.tmp` + `mv` for full rewrites, `>>` for single-line appends).
- **Hot-reload config.** Daemon re-reads `interval_minutes` every loop
  iteration — `set-interval` takes effect without restart.

---

## Verify

```bash
./verify.sh
```

Reports PASS for: tools, installed files, redaction self-test, stop-hook
smoke, tick.sh dry-run, settings.json hook integration, scheduler.

---

## Documentation

- [CHANGELOG.md](CHANGELOG.md) — release notes (v0.4.0 highlights)
- [DOCS-KO.md](DOCS-KO.md) — 한국어 사용자 문서
- [WSL-SETUP.md](WSL-SETUP.md) — Windows / WSL2 manual setup
- [systemd/README.md](systemd/README.md) — Linux systemd templates
- [CONTRIBUTING.md](CONTRIBUTING.md) — dev setup, testing, PR checklist

---

## Requirements

- Claude Code installed and authenticated
- `jq`, `python3`, `bash` (3.2+ on macOS, 5.x on Linux/WSL2)
- One of: `cron` / `launchd` / `systemd user` / Windows Task Scheduler
- (For source build) Rust stable toolchain via [rustup](https://rustup.rs/)

Optional:

- `rich` (`pip install rich`) — for the `/dashboard tui` mode
- [everything-claude-code](https://github.com/affaan-m/everything-claude-code)
  marketplace plugin (highly recommended — that's the point)

---

## Compatibility

| OS                | Tested | Scheduler                                  | Binary target              |
| ----------------- | ------ | ------------------------------------------ | -------------------------- |
| macOS 13+ x86_64  | ✓      | launchd `KeepAlive=true` or cron           | `x86_64-apple-darwin`      |
| macOS 14+ arm64   | ✓      | launchd `KeepAlive=true` or cron           | `aarch64-apple-darwin`     |
| Ubuntu 22.04+     | ✓      | systemd user timer or cron                 | `x86_64-unknown-linux-gnu` |
| Windows 11 + WSL  | ✓      | Task Scheduler → `wsl.exe ... we-forgectl` | `x86_64-pc-windows-msvc`   |

---

## License

MIT — see [LICENSE](LICENSE).

---

## Related

- [everything-claude-code](https://github.com/affaan-m/everything-claude-code)
  — the marketplace plugin we-forge is designed to complement
- [Claude Code](https://docs.claude.com) — the CLI itself
