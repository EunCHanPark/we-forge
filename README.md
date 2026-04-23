# we-forge

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-blue)
![Bash](https://img.shields.io/badge/bash-3.2%2B-orange)
![Python](https://img.shields.io/badge/python-3.8%2B-blue)

> 24/7 background pattern-learning + ECC-marketplace orchestration layer for
> Claude Code. Watches what you actually do, dedupes against the
> [everything-claude-code](https://github.com/affaan-m/everything-claude-code)
> marketplace, and either **recommends an existing skill** or **synthesizes a
> new one** — quietly, while you work.

## Why

You install [Claude Code](https://docs.claude.com) plus the
[ECC marketplace](https://github.com/affaan-m/everything-claude-code)
(hundreds of skills) and find:

1. You re-derive the same workflow patterns every week.
2. You don't know which marketplace skills cover what you're doing.
3. The official `continuous-learning-v2` instinct system isn't enough on its
   own to *cross-reference* your activity against the marketplace.

we-forge sits on top:

```
Your Claude Code session
       │
       │ Stop / SubagentStop hook  +  bash history sweep  +  transcript replay
       ▼
~/.claude/learning/data/events.jsonl
       │  hourly tick
       ▼
patterns.jsonl  (≥3 occurrences across ≥3 distinct sessions)
       │
       ▼
pattern-detector  ── dedupes against ──►  ECC marketplace (944 skills)
       │                                 ECC homunculus instincts
       │                                 we-forge learned skills
       │
       ├─ exists in marketplace?  →  ECC_MATCH  →  recommend (no synthesis)
       └─ truly novel?            →  synthesize → audit → ~/.claude/skills/learned/
```

## Quick Start — one line, zero post-install steps

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/EunCHanPark/we-forge/main/install.sh | bash
```

This installs we-forge **and** registers the launchd / systemd service
automatically. After it finishes, the service is already running. No further
steps required.

### Windows

In PowerShell:

```powershell
iwr -useb https://raw.githubusercontent.com/EunCHanPark/we-forge/main/install.ps1 | iex
```

Verifies WSL2 → clones we-forge into WSL2 → runs `install.sh` (which
registers systemd inside WSL) → registers a Windows Task Scheduler job that
wakes WSL2 hourly. See [WSL-SETUP.md](WSL-SETUP.md) for Windows Server.

### From a clone (any OS)

```bash
git clone https://github.com/EunCHanPark/we-forge.git
cd we-forge
./install.sh --dry-run       # preview
./install.sh                 # install + auto-register
./verify.sh                  # confirm
```

### One-line uninstall

```bash
we-forgectl uninstall          # safety-backup, stop, remove service
we-forgectl uninstall --deep   # also move ~/.we-forge and learning/data to backup
```

Backups always go to `~/.we-forge/backup/<ISO-timestamp>/` — nothing is
deleted, only moved. Restore by `mv`-ing back.

## Service control — `we-forgectl`

The single CLI for the whole lifecycle:

```bash
we-forgectl status                    # service state + mode + telegram
we-forgectl start | stop | restart    # lifecycle
we-forgectl tui                       # rich-powered control TUI
we-forgectl dashboard                 # open http://127.0.0.1:8765 dashboard
we-forgectl logs                      # tail recent ticks
we-forgectl doctor                    # diagnose dependencies
we-forgectl notify-test               # send Telegram ping (if enabled)
we-forgectl install --enable-telegram # opt-in Telegram bot (daemon mode)
we-forgectl uninstall                 # safety-backup → remove
```

## Use inside Claude Code

```
/skill-report     # 6-section report: telemetry, top patterns, ECC matches, learned, decisions
/dashboard        # web dashboard at http://127.0.0.1:8765
/dashboard tui    # rich-powered terminal UI (pip install rich)
/dashboard once   # one-shot stdout snapshot, no deps
/watch-and-learn  # manually trigger the synthesize-and-audit loop
```

## Telegram notifier (optional)

Opt in for Telegram alerts on key events (new skill synthesized, ECC_MATCH
diversion, daemon crash). Switches we-forge to **daemon mode** (long-running
process; `KeepAlive=true` on macOS, `Restart=always` on Linux):

```bash
# 1. Get a bot token from @BotFather, then your chat id from @userinfobot
export WE_FORGE_TELEGRAM_TOKEN=...
export WE_FORGE_TELEGRAM_CHAT_ID=...

# 2. Re-install in daemon mode
we-forgectl install --enable-telegram

# 3. Verify
we-forgectl notify-test
```

Bot commands (sent to your chat):
- `/skill_report` — top patterns + ECC marketplace matches
- `/last_tick` — tail of recent tick.log
- `/status` — service status
- `/dashboard` — dashboard URL

Disable without uninstalling: `we-forgectl install` (re-runs without
`--enable-telegram` reverts to scheduled mode).

## Verify

```bash
./verify.sh
```

Reports PASS for: tools, installed files, redaction self-test, stop-hook
smoke, tick.sh dry-run, settings.json hook integration, scheduler.

## What gets installed

```
~/.claude/
├── agents/                  monitor-sentinel · pattern-detector · skill-synthesizer · quality-auditor · we-forge
├── commands/                /skill-report · /watch-and-learn · /dashboard · /ask-codex · /ask-gemini
├── hooks/stop-telemetry.sh  Stop / SubagentStop hook (always exits 0)
├── learning/
│   ├── tick.sh              hourly entry point (bash-first hot path)
│   ├── normalize.py         canonicalization + promotion rule
│   ├── redact.sh            secret filter (--self-test)
│   └── data/                events / patterns / queue / ledger / state
└── skills/learned/          synthesized skills (auditor-passed)
```

Plus the merged Stop-hook entry in `~/.claude/settings.json`
(existing entries preserved, original backed up to `settings.json.bak.<ISO>`).

## Architecture highlights

- **Bash-first hot path.** `tick.sh` only spends API credits when the
  promotion queue is non-empty.
- **Secrets dropped, not masked.** `redact.sh` and `normalize.py` share a
  regex + Shannon-entropy filter.
- **ECC-first dedup.** `pattern-detector` checks 4 sources before queueing
  any candidate: `~/.claude/skills/learned/`, ECC marketplace, ECC instincts,
  ECC evolved skills.
- **ECC_MATCH diversion.** When a recurring pattern matches an existing
  marketplace skill, we-forge records a recommendation in its memory instead
  of synthesizing a duplicate.
- **Single-instance lock.** `tick.sh` uses portable `mkdir`-based locking;
  cron/launchd/systemd double-fires are safe no-ops.
- **Idempotent everything.** `install.sh`, `tick.sh`, `verify.sh` are all
  safe to re-run.

## Documentation

- [WSL-SETUP.md](WSL-SETUP.md) — Windows / WSL2 manual setup
- [DOCS-KO.md](DOCS-KO.md) — 한국어 사용자 문서
- [systemd/README.md](systemd/README.md) — Linux systemd templates
- [CONTRIBUTING.md](CONTRIBUTING.md) — dev setup, testing, PR checklist
- [CHANGELOG.md](CHANGELOG.md) — release notes

## Requirements

- `jq`, `python3`, `bash` (3.2+ on macOS, 5.x on Linux/WSL2)
- [Claude Code](https://docs.claude.com) installed and authenticated
- `cron` / `launchd` / `systemd` / Windows Task Scheduler for the hourly tick

Optional:

- `rich` (`pip install rich`) — for the `/dashboard tui` mode
- [everything-claude-code](https://github.com/affaan-m/everything-claude-code)
  marketplace plugin (highly recommended — that's the point)

## Compatibility

| OS               | Tested | Scheduler                                      |
| ---------------- | ------ | ---------------------------------------------- |
| macOS 13+        | ✓      | launchd (template) or cron                     |
| Ubuntu 22.04+    | ✓      | systemd user timer (template) or cron          |
| Windows 11 + WSL | ✓      | Windows Task Scheduler → `wsl.exe ... tick.sh` |
| Windows Server   | ✓      | same as Windows 11 — see [WSL-SETUP.md](WSL-SETUP.md) |

## License

MIT — see [LICENSE](LICENSE).

## Related

- [everything-claude-code](https://github.com/affaan-m/everything-claude-code)
  — the marketplace plugin we-forge is designed to complement
- [Claude Code](https://docs.claude.com) — the CLI itself
