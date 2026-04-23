#!/usr/bin/env bash
# stop-telemetry.sh — Claude Code Stop-hook sidecar.
#
# Input:  Stop-hook JSON on stdin:
#   { "session_id": "...", "transcript_path": "...",
#     "stop_hook_active": bool, "cwd": "..." }
#
# Side effects:
#   - reads ~/.bash_history delta since state.json.last_bash_offset
#   - reads tool-use entries from $transcript_path since
#     state.json.last_transcript_offset[session_id]
#   - drops lines via redact_line (never masks — drops the whole event)
#   - appends one JSON object per event to events.jsonl
#
# Invariants:
#   - ALWAYS exits 0 so Stop is never blocked.
#   - Errors are swallowed; diagnostics go to ~/.claude/learning/data/telemetry.log.
#
# Co-exists with ~/.claude/stop-hook-git-check.sh as a second entry in the
# Stop matcher array.

set -u
# deliberately no -e: we must survive every failure and still exit 0

CLAUDE_HOME="${CLAUDE_HOME:-$HOME/.claude}"
DATA_DIR="${CLAUDE_LEARNING_DATA:-$CLAUDE_HOME/learning/data}"
EVENTS="$DATA_DIR/events.jsonl"
STATE="$DATA_DIR/state.json"
LOG="$DATA_DIR/telemetry.log"
BASH_HIST="${BASH_HISTFILE_OVERRIDE:-$HOME/.bash_history}"

REDACT_LIB="${CLAUDE_REDACT_LIB:-$CLAUDE_HOME/learning/redact.sh}"

mkdir -p "$DATA_DIR" 2>/dev/null || true

_log() {
  printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >> "$LOG" 2>/dev/null || true
}
_now() { date -u +%Y-%m-%dT%H:%M:%SZ; }

_json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\t'/\\t}"
  s="${s//$'\r'/\\r}"
  s="${s//$'\n'/\\n}"
  printf '%s' "$s"
}

_emit_event() {
  # $1 source  $2 session_id  $3 raw
  local source="$1" sid="$2" raw="$3"
  [ -z "$raw" ] && return 0
  if ! printf '%s\n' "$raw" | bash "$REDACT_LIB" --check >/dev/null 2>&1; then
    # DROP
    return 0
  fi
  local esc
  esc="$(_json_escape "$raw")"
  printf '{"ts":"%s","session_id":"%s","source":"%s","raw":"%s","normalized":null}\n' \
    "$(_now)" "$(_json_escape "$sid")" "$source" "$esc" \
    >> "$EVENTS" 2>/dev/null || true
}

_state_get() {
  [ -f "$STATE" ] || { echo "0"; return 0; }
  python3 - "$STATE" "$1" <<'PY' 2>/dev/null || echo "0"
import json, sys
path, key = sys.argv[1], sys.argv[2]
try:
    with open(path) as f:
        st = json.load(f)
except Exception:
    print(0); sys.exit(0)
if "." in key:
    a, b = key.split(".", 1)
    v = st.get(a, {})
    v = v.get(b, 0) if isinstance(v, dict) else 0
    print(v)
else:
    print(st.get(key, 0))
PY
}

_state_set() {
  python3 - "$STATE" "$1" "$2" <<'PY' 2>/dev/null || true
import json, sys, os
path, key, value = sys.argv[1], sys.argv[2], int(sys.argv[3])
try:
    with open(path) as f:
        st = json.load(f)
except Exception:
    st = {}
if "." in key:
    a, b = key.split(".", 1)
    if not isinstance(st.get(a), dict):
        st[a] = {}
    st[a][b] = value
else:
    st[key] = value
tmp = path + ".tmp"
with open(tmp, "w") as f:
    json.dump(st, f)
os.replace(tmp, path)
PY
}

_parse_json_field() {
  # $1 json blob  $2 field name
  printf '%s' "$1" | python3 -c '
import json, sys
try:
    obj = json.load(sys.stdin)
except Exception:
    print(""); sys.exit(0)
print(obj.get(sys.argv[1], "") if isinstance(obj, dict) else "")
' "$2" 2>/dev/null
}

_main() {
  local hook_json
  hook_json="$(cat || true)"

  local session_id transcript_path
  session_id="$(_parse_json_field "$hook_json" session_id)"
  transcript_path="$(_parse_json_field "$hook_json" transcript_path)"
  [ -z "$session_id" ] && session_id="unknown"

  # ---- bash history delta
  if [ -r "$BASH_HIST" ]; then
    local total_lines last_off
    total_lines=$(wc -l < "$BASH_HIST" 2>/dev/null || echo 0)
    total_lines=${total_lines// /}
    last_off="$(_state_get last_bash_offset)"
    last_off=${last_off//[^0-9]/}
    last_off=${last_off:-0}
    if [ "${total_lines:-0}" -gt "$last_off" ] 2>/dev/null; then
      local delta=$((total_lines - last_off))
      while IFS= read -r line; do
        # zsh extended history: ": 1700000000:0;command"
        local cmd="${line#: *:*;}"
        [ -z "$cmd" ] && continue
        case "$cmd" in '#'*) continue;; esac
        _emit_event "bash" "$session_id" "$cmd"
      done < <(tail -n "$delta" "$BASH_HIST" 2>/dev/null)
      _state_set last_bash_offset "$total_lines"
    fi
  fi

  # ---- transcript tool-use delta
  if [ -n "$transcript_path" ] && [ -r "$transcript_path" ]; then
    local key="last_transcript_offset.$session_id"
    local last_off total_lines
    last_off="$(_state_get "$key")"
    last_off=${last_off//[^0-9]/}
    last_off=${last_off:-0}
    total_lines=$(wc -l < "$transcript_path" 2>/dev/null || echo 0)
    total_lines=${total_lines// /}
    if [ "${total_lines:-0}" -gt "$last_off" ] 2>/dev/null; then
      local delta=$((total_lines - last_off))
      while IFS= read -r tu_json; do
        _emit_event "transcript" "$session_id" "$tu_json"
      done < <(tail -n "$delta" "$transcript_path" 2>/dev/null | python3 -c '
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
' 2>/dev/null)
      _state_set "$key" "$total_lines"
    fi
  fi

  # No stophook heartbeat emitted: a literal "Stop fired" event repeated on
  # every session end would itself become a 3x-promoted pattern and synthesize
  # a bogus SKILL. Liveness can be inferred from telemetry.log timestamps.
}

_main >/dev/null 2>>"$LOG" || _log "stop-telemetry: main() failed (continuing)"
exit 0
