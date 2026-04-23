# we-forge

24/7 monitoring + repetitive pattern auto-learning system for the
ECC harness. Captures bash history, Claude transcripts, and Stop-hook
signals, promotes patterns seen ≥ 3 times across ≥ 3 distinct sessions
into candidate SKILL.md drafts, gates them through a quality auditor,
and auto-registers PASS drafts into `~/.claude/skills/learned/`.

## Repo layout

```
.
├── install.sh                       # idempotent installer; jq-merges settings.json
├── crontab.example                  # paste-into-crontab hourly entry
├── agents/
│   ├── monitor-sentinel.md          # read-only telemetry summarizer
│   ├── pattern-detector.md          # queue dedupe + cluster
│   ├── skill-synthesizer.md         # draft SKILL.md under pending/
│   └── quality-auditor.md           # PASS / REVISE / REJECT gate
├── commands/
│   ├── watch-and-learn.md           # orchestrator (team path + sequential fallback)
│   └── skill-report.md              # read-only viewer
├── hooks/
│   └── stop-telemetry.sh            # Stop-hook sidecar; always exits 0
└── learning/
    ├── tick.sh                      # cron entry point (bash-first hot path)
    ├── redact.sh                    # shared secret filter with --self-test
    ├── normalize.py                 # canonicalization + promotion rule
    └── settings.snippet.json        # Stop-hook fragment for the jq merge
```

After `./install.sh` the repo files are mirrored under `~/.claude/`.

## Install

```bash
# Prerequisites: jq, python3, bash
./install.sh            # idempotent; backs up settings.json first
./install.sh --dry-run  # preview actions without writing
./install.sh --test     # redact self-test + tick.sh dry-run fixture
```

Then install the hourly cron job (not automatic — cron is host-level state):

```bash
crontab -e
# paste the line from crontab.example
```

The installer extends the existing Stop-hook matcher in
`~/.claude/settings.json` so the telemetry hook runs **alongside** any
hook you already have (e.g. `stop-hook-git-check.sh`). A timestamped
backup is saved to `settings.json.bak.<ISO>`.

### macOS: grant cron Full Disk Access

On macOS (Sonoma+), `cron` cannot read `~/.bash_history`, `~/.claude/projects/*`,
or write to `~/.claude/learning/data/` without **Full Disk Access**. Without
it, `tick.sh` runs hourly but silently captures nothing.

1. Open **System Settings → Privacy & Security → Full Disk Access**.
2. Click the `+` button.
3. Press `Cmd+Shift+.` to reveal hidden directories, navigate to `/usr/sbin/`,
   and select `cron`. (If a dialog blocks `/usr/sbin`, type the path via
   `Cmd+Shift+G`.)
4. Ensure the toggle next to `cron` is on.
5. Verify after the next `:00` tick:

   ```bash
   tail -n 20 ~/.claude/learning/data/tick.log
   # Expect lines like: "[...] bash delta: +N lines (total=M)"
   # If you see only "tick begin"/"tick end" with no delta, FDA is missing.
   ```

If cron is not present on `/usr/sbin/cron` (some minimal installs), the
equivalent daemon is `launchd`. In that case use a `launchctl` plist
instead of `crontab` — not covered here.

## How it works

```
Stop hook ──► events.jsonl ──► tick.sh ──► normalize.py ──► patterns.jsonl
                                                               │
                                             ≥3x, ≥3 distinct sessions
                                                               ▼
                                                    promotion_queue.jsonl
                                                               │
                                                  claude -p /watch-and-learn
                                                               ▼
                                   pattern-detector → synthesizer ‖ auditor
                                                               │
                                                        PASS / REVISE / REJECT
                                                               ▼
                                               ~/.claude/skills/learned/<slug>/
```

Key properties:

- **Bash-first hot path.** `tick.sh` spends API credits only when
  `promotion_queue.jsonl` is non-empty.
- **Secrets are dropped, not masked.** `redact.sh` and `normalize.py`
  share the same regex + Shannon-entropy rules.
- **Parallel pipeline stages when available.** `/watch-and-learn` uses
  `TeamCreate`/`team_name` (gated on `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`)
  to fan candidates out concurrently; falls back to sequential `Agent` calls
  otherwise.
- **Single-instance lock.** `tick.sh` uses a portable `mkdir`-based lock
  so cron double-fires are safe no-ops.
- **Idempotent.** Re-running `install.sh`, `tick.sh`, and `/watch-and-learn`
  all converge on the same end state.

## Verify

On the ECC host after install:

```bash
# 1. Redaction unit test
bash ~/.claude/learning/redact.sh --self-test

# 2. Stop-hook smoke
echo '{"session_id":"t","transcript_path":"/dev/null","stop_hook_active":false,"cwd":"/tmp"}' \
  | ~/.claude/hooks/stop-telemetry.sh; echo "exit=$?"

# 3. Dry-run promotion test
CLAUDE_DRY_RUN=1 ~/.claude/learning/tick.sh
cat ~/.claude/learning/data/tick.log

# 4. Full loop (once you have ≥3 distinct sessions of a real pattern)
~/.claude/learning/tick.sh
ls ~/.claude/skills/learned/
cat ~/.claude/learning/data/ledger.jsonl

# 5. Interactive report (inside a Claude Code session)
/skill-report
```

## Data files

All under `~/.claude/learning/data/`:

| file                     | role                                                |
| ------------------------ | --------------------------------------------------- |
| `events.jsonl`           | raw captured events (bash, transcript, stophook)    |
| `patterns.jsonl`         | canonicalized pattern frequency table               |
| `promotion_queue.jsonl`  | patterns awaiting synthesis                         |
| `ledger.jsonl`           | auditor decisions (PASS / REVISE / REJECT)          |
| `rejected.txt`           | poison list — patterns that must never be re-queued |
| `state.json`             | cursors for bash-history and transcript deltas      |
| `tick.log`               | diagnostic log for the hourly cron                  |
| `telemetry.log`          | diagnostic log for the Stop hook                    |

Every event record uses this shape:

```json
{"ts":"2026-04-23T12:00:00Z","session_id":"sess-A","source":"bash","raw":"git status","normalized":"git status"}
```

Timestamps are ISO-8601 UTC throughout.

## Requirements

- `jq`, `python3`, `bash`
- Claude Code with `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` (already
  enabled in the target user's settings; the code degrades gracefully
  without it — `/watch-and-learn` uses sequential `Agent` calls when the
  team APIs are not present)
- `cron` (or an equivalent scheduler)
- Byte budget: `events.jsonl` auto-rotates at 50 MiB, keeping up to 3
  generations (~200 MiB worst case)
