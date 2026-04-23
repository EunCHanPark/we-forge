# Changelog

All notable changes to we-forge are documented in this file. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] — 2026-04-23

Initial public release.

### Highlights

- 24/7 background pattern-learning loop for Claude Code, designed to
  complement (not duplicate) the
  [everything-claude-code](https://github.com/affaan-m/everything-claude-code)
  marketplace.
- Cross-platform installers — macOS, Linux, WSL2 via `install.sh`; Windows
  via `install.ps1` (auto-registers Task Scheduler job).
- Dashboard with web (`--serve`) and TUI (`--tui`) modes.

### Added

#### Learning loop

- `agents/we-forge.md` — main-session orchestrator with persistent memory at
  `~/.claude/agent-memory/we-forge/`.
- `agents/pattern-detector.md` — reduces the promotion queue to distinct
  candidates, dedupes against **four** sources: we-forge learned skills,
  ECC marketplace skills (~944), ECC homunculus instincts, and ECC evolved
  skills.
- `agents/skill-synthesizer.md` — drafts SKILL.md following the ECC
  convention.
- `agents/quality-auditor.md` — gates drafts with a 6-rubric PASS/REVISE/
  REJECT decision; rejects suspicious-action drafts (URLs, sudo, eval, etc.)
  outright.
- `agents/monitor-sentinel.md` — read-only telemetry summarizer.
- `learning/tick.sh` — hourly cron entry point with portable mkdir-based
  single-instance lock and bash-first hot path.
- `learning/normalize.py` — pattern canonicalization (strips numbers, paths,
  hex, UUIDs to placeholders) + promotion rule (≥3 occurrences across ≥3
  distinct sessions).
- `learning/redact.sh` — secret filter with `--self-test`; drops (not masks)
  values matching API key / JWT / private key patterns or high Shannon
  entropy.
- `hooks/stop-telemetry.sh` — Stop / SubagentStop hook that captures session
  telemetry; always exits 0.

#### ECC integration

- `pattern-detector` now checks `~/.claude/plugins/marketplaces/**/SKILL.md`
  and `~/.claude/homunculus/projects/*/instincts/personal/*.yaml` to dedupe
  against the marketplace and ECC instincts.
- New `ECC_MATCH` decision type — when a candidate matches an existing
  marketplace skill, we-forge records a recommendation in `MEMORY.md` and
  the ledger instead of dispatching `skill-synthesizer`.

#### Slash commands

- `/skill-report` — six-section report (telemetry, patterns in flight,
  TOP 10 frequent patterns, ECC marketplace recommendations, learned skills,
  recent decisions).
- `/watch-and-learn` — manually trigger the synthesize-and-audit loop.
- `/dashboard [serve|tui|once]` — KPI dashboard launcher.
- `/ask-codex`, `/ask-gemini` — delegate one-shot questions to Codex/Gemini
  CLIs and return verbatim.

#### Dashboard

- `dashboard/dashboard.py` — single Python file, stdlib-only for `--serve`
  (HTTP on `127.0.0.1:8765` with Chart.js auto-refresh) and `--once` modes;
  optional `rich` dep for `--tui` mode.
- KPIs: events/day (7d), top patterns, ECC_MATCH ratio, marketplace
  recommendations, decision distribution doughnut, learned skills,
  dead-skill candidates (>14d).

#### Cross-platform installers

- `install.sh` — idempotent macOS/Linux/WSL2 installer with curl-pipe
  self-bootstrap (`curl ... | bash`), `--dry-run`, `--test`, `--branch`.
- `install.ps1` — Windows entry point. Verifies WSL2, clones we-forge into
  WSL2, runs `install.sh`, registers Windows Task Scheduler job that fires
  hourly via `wsl.exe -- bash ~/.claude/learning/tick.sh`.
- `verify.sh` — post-install self-test with 8 checks (tools, files, data
  dir, redact self-test, stop-hook smoke, tick dry-run, settings
  integration, scheduler).

#### Scheduler templates

- `crontab.example` — paste-into-crontab hourly entry.
- `launchd/com.we-forge-tick.plist.template` — macOS LaunchAgent.
- `systemd/we-forge-tick.{service,timer}.template` — Linux user-mode systemd
  timer with `OnBootSec=5min` catch-up + `OnUnitActiveSec=1h`.
- `systemd/README.md` — install / verify / uninstall guide.

#### Docs

- `README.md` — distribution-ready landing page with badges, per-OS one-line
  installs, architecture diagram, requirements, compatibility matrix.
- `WSL-SETUP.md` — Windows / WSL2 manual setup (with TL;DR pointing to
  `install.ps1` as the recommended path).
- `DOCS-KO.md` — 한국어 사용자 가이드.
- `CONTRIBUTING.md` — dev setup, coding conventions, PR checklist.
- `LICENSE` — MIT.

### Configuration changes

- ECC `continuous-learning-v2` observer enabled (`config.json`
  `observer.enabled: true`) so ECC instinct synthesis runs alongside
  we-forge.

### Notes

- `~/.claude/skills/learned/` is the only directory we-forge writes skills
  into. ECC instincts go to `~/.claude/homunculus/`. Both systems coexist
  without file conflicts.
- All event capture is local. No network calls from any agent.
- Secrets are dropped (not masked) by `redact.sh` before any data hits
  `events.jsonl` / `patterns.jsonl`.

[Unreleased]: https://github.com/EunCHanPark/we-forge/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/EunCHanPark/we-forge/releases/tag/v0.1.0
