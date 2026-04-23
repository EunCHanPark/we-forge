#!/usr/bin/env bash
# tick.sh — hourly cron entry point. Bash-only hot path.
#
# Pipeline:
#   1. Acquire an exclusive flock so cron double-fires don't collide.
#   2. Rotate events.jsonl if > 50 MB (keep 3 generations).
#   3. Snapshot bash-history delta as a fallback (covers sessions where
#      the Stop hook didn't fire, e.g. hard crash).
#   4. Run normalize.py (canonicalize + promote >=3x patterns).
#   5. If promotion_queue.jsonl has any unprocessed entries, invoke
#      `claude -p /watch-and-learn` with CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1.
#      This is the ONLY step that spends API credits.
#   6. Update state.json cursors.
#
# Env knobs (all optional):
#   CLAUDE_HOME           default: $HOME/.claude
#   CLAUDE_LEARNING_DATA  default: $CLAUDE_HOME/learning/data
#   CLAUDE_DRY_RUN=1      skip the claude -p invocation (for tests)
#   CLAUDE_TICK_TIMEOUT   default: 600 (seconds) for the claude invocation
#
# Exits 0 on success or benign no-ops; non-zero only on lock-file errors.

set -u
set -o pipefail

CLAUDE_HOME="${CLAUDE_HOME:-$HOME/.claude}"
DATA_DIR="${CLAUDE_LEARNING_DATA:-$CLAUDE_HOME/learning/data}"
LEARNING_DIR="${CLAUDE_LEARNING_DIR:-$CLAUDE_HOME/learning}"
REDACT_LIB="${CLAUDE_REDACT_LIB:-$LEARNING_DIR/redact.sh}"
NORMALIZE_PY="${CLAUDE_NORMALIZE_PY:-$LEARNING_DIR/normalize.py}"
# Auto-detect the shell history file:
#   1. explicit override wins (used by install.sh --test with a fixture)
#   2. ~/.bash_history if readable
#   3. ~/.zsh_history if readable (macOS default since Catalina)
#   4. fall back to ~/.bash_history (likely missing; handled by readability
#      guard downstream — silent no-op, not a crash)
# The zsh extended history format ": 1700000000:0;command" is already
# parsed by the `${line#: *:*;}` expansion below, so no format branch needed.
if [ -n "${BASH_HISTFILE_OVERRIDE:-}" ]; then
  BASH_HIST="$BASH_HISTFILE_OVERRIDE"
elif [ -r "$HOME/.bash_history" ]; then
  BASH_HIST="$HOME/.bash_history"
elif [ -r "$HOME/.zsh_history" ]; then
  BASH_HIST="$HOME/.zsh_history"
else
  BASH_HIST="$HOME/.bash_history"
fi

EVENTS="$DATA_DIR/events.jsonl"
QUEUE="$DATA_DIR/promotion_queue.jsonl"
STATE="$DATA_DIR/state.json"
LOCK="$DATA_DIR/.tick.lock"
LOG="$DATA_DIR/tick.log"

TIMEOUT="${CLAUDE_TICK_TIMEOUT:-600}"
MAX_EVENTS_BYTES="${CLAUDE_MAX_EVENTS_BYTES:-52428800}" # 50 MiB

mkdir -p "$DATA_DIR" 2>/dev/null || true
touch "$EVENTS" 2>/dev/null || true

_log() {
  printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >> "$LOG" 2>/dev/null || true
}
_now() { date -u +%Y-%m-%dT%H:%M:%SZ; }

_file_bytes() {
  if stat -f%z "$1" >/dev/null 2>&1; then
    stat -f%z "$1" 2>/dev/null
  else
    stat -c%s "$1" 2>/dev/null
  fi
}

_rotate_events() {
  local sz
  sz="$(_file_bytes "$EVENTS")"
  sz="${sz:-0}"
  if [ "$sz" -gt "$MAX_EVENTS_BYTES" ] 2>/dev/null; then
    _log "rotate: events.jsonl=${sz}b exceeds ${MAX_EVENTS_BYTES}b"
    [ -f "$EVENTS.2" ] && mv -f "$EVENTS.2" "$EVENTS.3" 2>/dev/null || true
    [ -f "$EVENTS.1" ] && mv -f "$EVENTS.1" "$EVENTS.2" 2>/dev/null || true
    mv -f "$EVENTS" "$EVENTS.1" 2>/dev/null || true
    : > "$EVENTS"
  fi
}

_emit_event() {
  # $1 source  $2 raw  $3 session_id (optional, default "cron")
  local source="$1" raw="$2" sid="${3:-cron}"
  [ -z "$raw" ] && return 0
  if ! printf '%s\n' "$raw" | bash "$REDACT_LIB" --check >/dev/null 2>&1; then
    return 0
  fi
  python3 -c '
import json, sys
ts, source, raw, sid = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
print(json.dumps({
    "ts": ts, "session_id": sid, "source": source,
    "raw": raw, "normalized": None
}))
' "$(_now)" "$source" "$raw" "$sid" >> "$EVENTS" 2>/dev/null || true
}

_transcript_catch_up() {
  # Fallback for sessions where Stop hook didn't fire (Claude Code crash,
  # SIGKILL, terminal close without clean exit, etc.). Scans all project
  # transcript .jsonl files under ~/.claude/projects/ and emits any
  # tool_use events we haven't already captured, keyed by session_id
  # from the transcript filename.
  local proj_dir="${CLAUDE_PROJECTS_DIR:-$HOME/.claude/projects}"
  [ -d "$proj_dir" ] || return 0
  local tpath sid total_lines last_off delta
  while IFS= read -r tpath; do
    [ -z "$tpath" ] && continue
    sid="$(basename "$tpath" .jsonl)"
    [ -z "$sid" ] && continue
    total_lines=$(wc -l < "$tpath" 2>/dev/null || echo 0)
    total_lines=${total_lines// /}
    last_off="$(python3 -c '
import json, sys
try:
    with open(sys.argv[1]) as f:
        st = json.load(f)
    v = st.get("last_transcript_offset", {}).get(sys.argv[2], 0)
    print(int(v) if isinstance(v, int) else 0)
except Exception:
    print(0)
' "$STATE" "$sid" 2>/dev/null)"
    last_off=${last_off//[^0-9]/}
    last_off=${last_off:-0}
    if [ "${total_lines:-0}" -gt "$last_off" ] 2>/dev/null; then
      delta=$((total_lines - last_off))
      _log "transcript catch-up: session=${sid:0:8} +$delta lines"
      tail -n "$delta" "$tpath" 2>/dev/null | python3 -c '
import json, sys
for ln in sys.stdin:
    ln = ln.strip()
    if not ln:
        continue
    try:
        obj = json.loads(ln)
    except Exception:
        continue
    msg = obj.get("message") if isinstance(obj, dict) else None
    if not isinstance(msg, dict):
        msg = obj
    content = msg.get("content") if isinstance(msg, dict) else None
    if isinstance(content, list):
        for c in content:
            if isinstance(c, dict) and c.get("type") == "tool_use":
                print(json.dumps({"tool": c.get("name"), "input": c.get("input", {})}))
' 2>/dev/null | while IFS= read -r tu_json; do
        _emit_event "transcript" "$tu_json" "$sid"
      done
      python3 -c '
import json, os, sys
path, key, value = sys.argv[1], sys.argv[2], int(sys.argv[3])
try:
    with open(path) as f: st = json.load(f)
except Exception:
    st = {}
if not isinstance(st.get("last_transcript_offset"), dict):
    st["last_transcript_offset"] = {}
st["last_transcript_offset"][key] = value
tmp = path + ".tmp"
with open(tmp, "w") as f: json.dump(st, f)
os.replace(tmp, path)
' "$STATE" "$sid" "$total_lines" 2>/dev/null || true
    fi
  done < <(find "$proj_dir" -maxdepth 3 -name "*.jsonl" -type f 2>/dev/null)
}

_bash_history_delta() {
  [ -r "$BASH_HIST" ] || return 0
  local total_lines last_off
  total_lines=$(wc -l < "$BASH_HIST" 2>/dev/null || echo 0)
  total_lines=${total_lines// /}
  last_off="$(python3 -c '
import json, sys
try:
    with open(sys.argv[1]) as f:
        st = json.load(f)
    print(st.get("last_bash_offset", 0))
except Exception:
    print(0)
' "$STATE" 2>/dev/null)"
  last_off=${last_off//[^0-9]/}
  last_off=${last_off:-0}

  if [ "${total_lines:-0}" -gt "$last_off" ] 2>/dev/null; then
    local delta=$((total_lines - last_off))
    _log "bash delta: +$delta lines (total=$total_lines)"
    while IFS= read -r line; do
      local cmd="${line#: *:*;}"
      [ -z "$cmd" ] && continue
      case "$cmd" in '#'*) continue;; esac
      _emit_event "bash" "$cmd"
    done < <(tail -n "$delta" "$BASH_HIST" 2>/dev/null)
    python3 -c '
import json, os, sys
path, key, value = sys.argv[1], sys.argv[2], int(sys.argv[3])
try:
    with open(path) as f: st = json.load(f)
except Exception:
    st = {}
st[key] = value
tmp = path + ".tmp"
with open(tmp, "w") as f: json.dump(st, f)
os.replace(tmp, path)
' "$STATE" last_bash_offset "$total_lines" 2>/dev/null || true
  fi
}

_queue_nonempty() {
  [ -s "$QUEUE" ]
}

_invoke_watch_and_learn() {
  if [ "${CLAUDE_DRY_RUN:-0}" = "1" ]; then
    _log "dry-run: skipping claude -p /watch-and-learn"
    return 0
  fi
  if ! command -v claude >/dev/null 2>&1; then
    _log "claude CLI not on PATH; skipping synthesis"
    return 0
  fi
  _log "invoking claude --agent we-forge -p tick (timeout=${TIMEOUT}s)"
  local tcmd=""
  if command -v timeout >/dev/null 2>&1; then tcmd="timeout ${TIMEOUT}"
  elif command -v gtimeout >/dev/null 2>&1; then tcmd="gtimeout ${TIMEOUT}"
  fi
  (
    export CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1
    # Disable ECC hooks that would block or duplicate work inside the headless
    # subprocess. Read by ECC's hook-flags.js (no ECC plugin files are modified):
    #   - pre:bash:gateguard-fact-force      — blocks first routine Bash + rm -rf
    #   - pre:edit-write:gateguard-fact-force — blocks first Write/Edit per file
    #   - pre:observe:continuous-learning    — duplicate-watch; we already capture
    # Scoped to this subshell only; parent shell / cron env are unaffected.
    export ECC_DISABLED_HOOKS="pre:bash:gateguard-fact-force,pre:edit-write:gateguard-fact-force,pre:observe:continuous-learning"
    # Run as the we-forge main-session agent so it has persistent memory at
    # ~/.claude/agent-memory/we-forge/. The /watch-and-learn slash command
    # remains available for interactive triggering; this headless path uses
    # the agent path to accumulate cross-run learnings.
    # --dangerously-skip-permissions: required so the agent can write to
    # ~/.claude/learning/data/ledger.jsonl, ~/.claude/learning/data/promotion_queue.jsonl,
    # and ~/.claude/skills/learned/ (all hardcoded "sensitive" paths in CC).
    # Without this flag headless ticks cannot clear the processed queue, so
    # the same stale entries re-circulate every tick and burn API spend.
    # See ~/.claude/agent-memory/we-forge/MEMORY.md "Persistent permission block"
    # for the diagnosis history.
    # shellcheck disable=SC2086
    $tcmd claude --dangerously-skip-permissions --agent we-forge -p "tick" >>"$LOG" 2>&1
  ) || _log "claude --agent we-forge returned non-zero (ignored)"
}

_main() {
  # Portable single-instance lock via mkdir (atomic on all POSIX fs).
  # Stale locks older than 2 * TIMEOUT seconds are cleaned up.
  local lockdir="${LOCK}.d"
  if ! mkdir "$lockdir" 2>/dev/null; then
    local lock_age=0
    if [ -d "$lockdir" ]; then
      local lock_mtime now
      if stat -f%m "$lockdir" >/dev/null 2>&1; then
        lock_mtime="$(stat -f%m "$lockdir" 2>/dev/null)"
      else
        lock_mtime="$(stat -c%Y "$lockdir" 2>/dev/null)"
      fi
      now="$(date +%s)"
      lock_age=$(( now - ${lock_mtime:-$now} ))
    fi
    local stale_after=$(( TIMEOUT * 2 ))
    if [ "$lock_age" -gt "$stale_after" ]; then
      _log "stale lock (age=${lock_age}s) — reclaiming"
      rm -rf "$lockdir" 2>/dev/null || true
      mkdir "$lockdir" 2>/dev/null || { _log "cannot reclaim lock"; return 1; }
    else
      _log "another tick is running (lock age=${lock_age}s); exiting"
      return 0
    fi
  fi
  # shellcheck disable=SC2064
  trap "rmdir '$lockdir' 2>/dev/null || true" EXIT INT TERM

  _log "tick begin"
  _rotate_events
  _bash_history_delta
  _transcript_catch_up

  if [ -f "$NORMALIZE_PY" ]; then
    CLAUDE_LEARNING_DATA="$DATA_DIR" \
    CLAUDE_LEARNED_SKILLS="${CLAUDE_LEARNED_SKILLS:-$CLAUDE_HOME/skills/learned}" \
      python3 "$NORMALIZE_PY" >>"$LOG" 2>&1 || _log "normalize.py failed"
  else
    _log "normalize.py missing at $NORMALIZE_PY"
  fi

  if _queue_nonempty; then
    _invoke_watch_and_learn
  else
    _log "promotion queue empty; no claude invocation"
  fi

  _log "tick end"
}

_main
