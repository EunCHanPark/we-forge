# Contributing to we-forge

Thanks for your interest in we-forge.

## Quick start (development)

```bash
git clone https://github.com/EunCHanPark/we-forge.git
cd we-forge

# Dry-run install to preview what changes
./install.sh --dry-run

# Real install (idempotent — safe to re-run)
./install.sh

# Verify
./verify.sh
```

## Project layout

| Path | Role |
| ---- | ---- |
| `install.sh` | Mac/Linux installer (idempotent, jq-merges settings.json) |
| `install.ps1` | Windows entry point (validates WSL2, delegates to install.sh) |
| `verify.sh` | Post-install self-test |
| `agents/` | Sub-agent definitions (markdown with YAML frontmatter) |
| `commands/` | Slash command definitions |
| `hooks/stop-telemetry.sh` | Stop-hook that captures session telemetry |
| `learning/tick.sh` | Hourly cron entry point (bash-first hot path) |
| `learning/normalize.py` | Pattern canonicalization + promotion rule |
| `learning/redact.sh` | Secret filter with `--self-test` |
| `dashboard/dashboard.py` | Web (`--serve`) and TUI (`--tui`) dashboard |
| `launchd/`, `systemd/`, `crontab.example` | Per-OS scheduler templates |

## Coding conventions

- **Bash is the hot path.** New event-loop logic stays in bash; only fall back
  to Python for canonicalization or non-trivial parsing.
- **Idempotent everything.** `install.sh`, `tick.sh`, `verify.sh` must be
  safe to re-run any number of times.
- **No secrets in commits.** All telemetry passes through `redact.sh` /
  `normalize.py`. Run `bash learning/redact.sh --self-test` before committing
  changes that touch redaction.
- **Cross-platform.** Bash scripts must run on macOS bash 3.2 (Apple ships old
  bash) AND Linux bash 5.x AND WSL2. Avoid bashisms unique to 4.x+ unless
  guarded.
- **No external network calls in agents.** Agents declare `tools:` minimally;
  no `WebFetch`/`WebSearch` in pattern-detector, synthesizer, auditor, etc.
  All work is local under `~/.claude/`.

## Testing

```bash
# Redaction unit test
bash learning/redact.sh --self-test

# tick.sh dry-run (no API spend, no writes)
CLAUDE_DRY_RUN=1 ./learning/tick.sh

# Full installer dry-run
./install.sh --dry-run

# Self-test the installed system
./verify.sh
```

## Commit style

Follow [Conventional Commits](https://www.conventionalcommits.org/) loosely:

```
feat: add ECC marketplace dedup to pattern-detector
fix: handle empty promotion_queue in tick.sh
docs: clarify Windows install steps
chore: bump verify.sh timeout
```

## PR checklist

- [ ] `./install.sh --dry-run` succeeds
- [ ] `bash learning/redact.sh --self-test` passes
- [ ] `./verify.sh` passes after a fresh install
- [ ] Updated `README.md` and/or `DOCS-KO.md` if user-facing behavior changed
- [ ] No raw event content (potentially containing secrets) in test fixtures
- [ ] Mirror copies under `~/.claude/agents/`, `~/.claude/commands/` updated
      if the change touches `agents/*.md` or `commands/*.md`

## Issue reports

Useful info to include:

- OS + version (macOS X.Y, Windows + WSL2 distro, Linux distro)
- `claude --version` output
- Output of `cat ~/.claude/learning/data/tick.log | tail -50`
- Output of `~/.claude/hooks/stop-telemetry.sh </dev/null; echo "exit=$?"`

## Code of conduct

Be kind. Assume good intent. We're all here to make Claude Code more useful.

## License

By contributing, you agree your contributions are licensed under the MIT
License (see `LICENSE`).
