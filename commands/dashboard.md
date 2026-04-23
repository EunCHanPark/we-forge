---
description: Open the we-forge KPI dashboard. Pass `serve` (default, web UI on localhost:8765), `tui` (rich-powered terminal UI, requires `pip install rich`), or `once` (print KPIs to stdout once and exit). Shows top patterns, ECC marketplace recommendations, auditor decisions, learned skills, and dead-skill candidates.
---

You are executing **/dashboard**, a launcher for the we-forge KPI dashboard.

## Argument

The command accepts ONE optional positional argument:

- `serve` (default) — start an HTTP server on `http://127.0.0.1:8765/`
  with auto-refresh. Auto-opens the user's default browser.
- `tui` — render a rich-powered live-refreshing terminal UI.
  Requires `rich`; falls back to `once` if not installed.
- `once` — print a one-shot snapshot to stdout and exit. No deps.

## Flow

1. Locate the dashboard script. Try in order:
   - `~/we-forge/dashboard/dashboard.py` (default install location)
   - `${WE_FORGE_REPO_DIR}/dashboard/dashboard.py` if env var set
   - `$(pwd)/dashboard/dashboard.py` if running from a checkout
   If none found, tell the user how to clone/install.

2. Invoke via Bash:
   ```bash
   python3 <path>/dashboard.py --<mode>
   ```
   For `serve`, the default `--port 8765` and browser auto-open apply
   unless the user has overrides via env (`WE_FORGE_PORT`).

3. For `serve` mode:
   - Run in background (Bash `run_in_background=true`) so the user can
     keep working in the same Claude Code session.
   - Print the URL and a one-line "Ctrl-C in the launching terminal to
     stop, or use `pkill -f dashboard.py`".

4. For `tui` mode:
   - Run in foreground. The user is dropped into the TUI; Ctrl-C exits.

5. For `once` mode:
   - Run in foreground; capture stdout and present back to user as a
     formatted block.

## Rules

- **Read-only.** The dashboard only reads `~/.claude/learning/data/*`,
  `~/.claude/skills/learned/*`, `~/.claude/plugins/marketplaces/**`,
  and `~/.claude/homunculus/projects/*`. It writes nothing.
- **Localhost only.** `--serve` binds 127.0.0.1; never expose externally.
- **No secrets.** The dashboard inherits `redact.sh`/`normalize.py`
  upstream filtering — it never re-introduces raw event content beyond
  what's already in `patterns.jsonl`.
- **Don't dispatch sub-agents.** This is a thin launcher.

## Examples

User: `/dashboard`
You: launch web dashboard in background, return URL.

User: `/dashboard tui`
You: launch TUI in foreground.

User: `/dashboard once`
You: capture and pretty-print the snapshot.
