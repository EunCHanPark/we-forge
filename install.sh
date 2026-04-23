#!/usr/bin/env bash
# install.sh — idempotent installer for the 24/7 pattern-learning system.
#
# Usage:
#   ./install.sh           # install into ~/.claude/ (default)
#   ./install.sh --test    # run redact self-test + tick.sh dry-run fixture
#   ./install.sh --dry-run # show what would happen, write nothing
#
# Env:
#   CLAUDE_HOME   override install prefix (default: $HOME/.claude)
#
# Side effects:
#   - mkdir the ECC dir tree under $CLAUDE_HOME
#   - copy agents/, commands/, hooks/, learning/ into place
#   - seed empty data files in $CLAUDE_HOME/learning/data/
#   - jq-merge the Stop-hook entry into $CLAUDE_HOME/settings.json
#     (existing entries preserved; previous file backed up to settings.json.bak.<ISO>)
#   - print the crontab line; does NOT modify the user's crontab

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_HOME="${CLAUDE_HOME:-$HOME/.claude}"

DRY_RUN=0
TEST_MODE=0

for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --test)    TEST_MODE=1 ;;
    -h|--help)
      sed -n '1,/^$/p' "$0" | sed -n 's/^# \{0,1\}//p'
      exit 0
      ;;
    *)
      echo "unknown flag: $arg" >&2
      exit 2
      ;;
  esac
done

_say()  { printf '==> %s\n' "$*"; }
_warn() { printf 'warn: %s\n' "$*" >&2; }
_die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

_run() {
  if [ "$DRY_RUN" = "1" ]; then
    printf '  DRY: %s\n' "$*"
  else
    eval "$@"
  fi
}

_copy() {
  local src="$1" dst="$2"
  [ -f "$src" ] || _die "missing source: $src"
  _run "mkdir -p \"$(dirname "$dst")\""
  _run "cp -f \"$src\" \"$dst\""
  _run "chmod +r \"$dst\""
  case "$src" in
    *.sh|*.py) _run "chmod +x \"$dst\"" ;;
  esac
}

# -----------------------------------------------------------------------
run_test_mode() {
  _say "test: redact self-test"
  bash "$REPO_DIR/learning/redact.sh" --self-test || _die "redact self-test failed"

  _say "test: tick.sh dry-run with 3-distinct-sessions fixture"
  local T
  T="$(mktemp -d)"
  mkdir -p "$T/claude/learning/data" "$T/claude/skills/learned"
  cat > "$T/claude/learning/data/events.jsonl" <<'EOF'
{"ts":"2026-04-23T12:00:00Z","session_id":"sess-A","source":"bash","raw":"git status","normalized":null}
{"ts":"2026-04-23T12:05:00Z","session_id":"sess-B","source":"bash","raw":"git status","normalized":null}
{"ts":"2026-04-23T12:10:00Z","session_id":"sess-C","source":"bash","raw":"git status","normalized":null}
{"ts":"2026-04-23T12:15:00Z","session_id":"sess-A","source":"bash","raw":"ANTHROPIC_API_KEY=sk-ant-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","normalized":null}
EOF
  touch "$T/empty_history"
  CLAUDE_HOME="$T/claude" \
    CLAUDE_LEARNING_DATA="$T/claude/learning/data" \
    CLAUDE_LEARNING_DIR="$T/claude/learning" \
    CLAUDE_LEARNED_SKILLS="$T/claude/skills/learned" \
    CLAUDE_REDACT_LIB="$REPO_DIR/learning/redact.sh" \
    CLAUDE_NORMALIZE_PY="$REPO_DIR/learning/normalize.py" \
    BASH_HISTFILE_OVERRIDE="$T/empty_history" \
    CLAUDE_DRY_RUN=1 \
    bash "$REPO_DIR/learning/tick.sh"

  if ! grep -q '"pattern": "git status"' "$T/claude/learning/data/promotion_queue.jsonl" 2>/dev/null; then
    _die "tick.sh did not promote 'git status' to the queue"
  fi
  # events.jsonl is append-only (stop-telemetry's redact filter is the sole
  # gate on what gets written). normalize.py does not scrub events.jsonl in
  # place — doing so would race with stop-telemetry's O_APPEND fd and lose
  # in-flight events. The real invariant is that no secret propagates
  # DOWNSTREAM (patterns.jsonl, promotion_queue.jsonl, learned/).
  if grep -q 'sk-ant' "$T/claude/learning/data/patterns.jsonl" 2>/dev/null; then
    _die "secret leaked into patterns.jsonl"
  fi
  if grep -q 'sk-ant' "$T/claude/learning/data/promotion_queue.jsonl" 2>/dev/null; then
    _die "secret leaked into promotion_queue.jsonl"
  fi

  _say "test: PASS"
  echo "(tmpdir left at $T for inspection; safe to remove manually)"
  exit 0
}

[ "$TEST_MODE" = "1" ] && run_test_mode

# -----------------------------------------------------------------------
command -v jq >/dev/null 2>&1 || _die "jq is required; install with: brew install jq (or apt-get install jq)"
command -v python3 >/dev/null 2>&1 || _die "python3 is required"

_say "installing into $CLAUDE_HOME (dry-run=$DRY_RUN)"

_run "mkdir -p \"$CLAUDE_HOME/agents\" \"$CLAUDE_HOME/commands\" \"$CLAUDE_HOME/hooks\" \"$CLAUDE_HOME/learning/data\" \"$CLAUDE_HOME/skills/learned\""

_copy "$REPO_DIR/learning/redact.sh"    "$CLAUDE_HOME/learning/redact.sh"
_copy "$REPO_DIR/learning/normalize.py" "$CLAUDE_HOME/learning/normalize.py"
_copy "$REPO_DIR/learning/tick.sh"      "$CLAUDE_HOME/learning/tick.sh"

_copy "$REPO_DIR/hooks/stop-telemetry.sh" "$CLAUDE_HOME/hooks/stop-telemetry.sh"

for a in monitor-sentinel pattern-detector skill-synthesizer quality-auditor; do
  _copy "$REPO_DIR/agents/$a.md" "$CLAUDE_HOME/agents/$a.md"
done

for c in watch-and-learn skill-report; do
  _copy "$REPO_DIR/commands/$c.md" "$CLAUDE_HOME/commands/$c.md"
done

for f in events.jsonl patterns.jsonl promotion_queue.jsonl ledger.jsonl rejected.txt; do
  _run "touch \"$CLAUDE_HOME/learning/data/$f\""
done
if [ ! -f "$CLAUDE_HOME/learning/data/state.json" ]; then
  _run "printf '%s' '{}' > \"$CLAUDE_HOME/learning/data/state.json\""
fi

# Settings merge (jq)
SETTINGS="$CLAUDE_HOME/settings.json"
SNIPPET="$REPO_DIR/learning/settings.snippet.json"
BACKUP="$SETTINGS.bak.$(date -u +%Y%m%dT%H%M%SZ)"

if [ ! -f "$SETTINGS" ]; then
  _say "no existing settings.json — creating fresh from snippet"
  _run "cp \"$SNIPPET\" \"$SETTINGS\""
else
  _say "backing up settings.json → $BACKUP"
  _run "cp -f \"$SETTINGS\" \"$BACKUP\""

  _say "merging Stop-hook telemetry entry via jq"
  MERGE_EXPR='
    .hooks //= {} |
    .hooks.Stop //= [] |
    .hooks.Stop |= (
      if (length == 0) then
        [{matcher:"", hooks:[{type:"command", command:"~/.claude/hooks/stop-telemetry.sh"}]}]
      else
        ( map(
            if (.matcher == "" or .matcher == null)
            then .hooks = (
              (.hooks // [])
              | if (map(.command) | index("~/.claude/hooks/stop-telemetry.sh"))
                then .
                else . + [{type:"command", command:"~/.claude/hooks/stop-telemetry.sh"}]
                end
            )
            else .
            end
          )
        ) as $arr
        | if ($arr | any(.matcher == "" or .matcher == null))
          then $arr
          else $arr + [{matcher:"", hooks:[{type:"command", command:"~/.claude/hooks/stop-telemetry.sh"}]}]
          end
      end
    )
  '
  if [ "$DRY_RUN" = "1" ]; then
    echo "  DRY: jq merge preview:"
    jq "$MERGE_EXPR" "$SETTINGS"
  else
    TMP="$(mktemp)"
    jq "$MERGE_EXPR" "$SETTINGS" > "$TMP"
    mv "$TMP" "$SETTINGS"
  fi
fi

_say "install complete."

if ! find "$CLAUDE_HOME/projects" -maxdepth 3 -name '*.jsonl' 2>/dev/null | head -1 | grep -q .; then
  _warn "no transcript .jsonl files found under $CLAUDE_HOME/projects/ yet."
  _warn "This is fine on a fresh install — the Stop hook will start logging on the first Claude session."
fi

cat <<EON

Next steps:

  1. Review the merged settings.json:
       jq .hooks.Stop "$SETTINGS"

  2. Install the cron entry (not automatic):
       crontab -e
     then paste the line from:
       $REPO_DIR/crontab.example

  3. Verify:
       bash $CLAUDE_HOME/learning/redact.sh --self-test
       echo '{"session_id":"t","transcript_path":"/dev/null","stop_hook_active":false,"cwd":"/tmp"}' \\
         | $CLAUDE_HOME/hooks/stop-telemetry.sh; echo "exit=\$?"

  4. Full end-to-end test:
       $REPO_DIR/install.sh --test

EON
