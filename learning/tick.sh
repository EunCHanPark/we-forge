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
BASH_HIST="${BASH_HISTFILE_OVERRIDE:-$HOME/.bash_history}"

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
  # $1 source  $2 raw  — session_id is "cron" for fallback-captured lines.
  local source="$1" raw="$2"
  [ -z "$raw" ] && return 0
  if ! printf '%s\n' "$raw" | bash "$REDACT_LIB" --check >/dev/null 2>&1; then
    return 0
  fi
  python3 -c '
import json, sys
ts, source, raw = sys.argv[1], sys.argv[2], sys.argv[3]
print(json.dumps({
    "ts": ts, "session_id": "cron", "source": source,
    "raw": raw, "normalized": None
}))
' "$(_now)" "$source" "$raw" >> "$EVENTS" 2>/dev/null || true
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
  _log "invoking claude -p /watch-and-learn (timeout=${TIMEOUT}s)"
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
    # shellcheck disable=SC2086
    $tcmd claude -p "/watch-and-learn" >>"$LOG" 2>&1
  ) || _log "claude -p returned non-zero (ignored)"
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
