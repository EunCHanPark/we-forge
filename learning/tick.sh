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
#   CLAUDE_TICK_TIMEOUT   default: 900 (seconds) for the claude invocation
#                         (raised from 600: the orchestrator now makes extra
#                          sub-agent round-trips — memory-manager load+record,
#                          notifier — which pushed slow ticks near the old cap)
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

TIMEOUT="${CLAUDE_TICK_TIMEOUT:-900}"
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

  # Sequence-pattern extractor (shadow mode — emits SEQ_CANDIDATE only).
  # Independent of normalize.py: reads the same events.jsonl but groups
  # by session + 5-min window to surface multi-step workflows that the
  # single-event normalizer cannot see.
  local seq_py="$CLAUDE_HOME/learning/sequence_normalize.py"
  if [ -f "$seq_py" ]; then
    CLAUDE_LEARNING_DATA="$DATA_DIR" \
      python3 "$seq_py" >>"$LOG" 2>&1 || _log "sequence_normalize.py failed"
  fi

  # Refresh ECC keyword index if older than 24h or missing.
  local idx="${WE_FORGE_HOME:-$HOME/.we-forge}/ecc-index.json"
  local idx_builder="$CLAUDE_HOME/learning/build_ecc_index.py"
  if [ -f "$idx_builder" ]; then
    local idx_mtime now stale=1
    if [ -f "$idx" ]; then
      if stat -f%m "$idx" >/dev/null 2>&1; then
        idx_mtime="$(stat -f%m "$idx")"
      else
        idx_mtime="$(stat -c%Y "$idx" 2>/dev/null)"
      fi
      now="$(date +%s)"
      [ $(( now - ${idx_mtime:-0} )) -lt 86400 ] && stale=0
    fi
    if [ "$stale" = "1" ]; then
      python3 "$idx_builder" >>"$LOG" 2>&1 || _log "ecc-index rebuild failed (builder exited non-zero; existing index untouched)"
    fi

    # Integrity gate: even with fresh mtime, verify the on-disk index actually
    # has the shape the Rust skill-suggest matcher requires. The builder's own
    # self-validation catches a degenerate rebuild, but this catches stale
    # corrupt indexes from older builder versions or out-of-band edits.
    if [ -f "$idx" ]; then
      python3 - "$idx" <<'PYEOF' >>"$LOG" 2>&1 || _log "ecc-index integrity check failed — skill-suggest will return 'no match' for every prompt (rebuild via: python3 learning/build_ecc_index.py)"
import json, sys
try:
    idx = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception as e:
    print(f"ecc-index integrity: unreadable ({e})", file=sys.stderr); sys.exit(1)
marketplace = sum(1 for s in idx.get("skills", []) if s.get("source") == "marketplace")
if marketplace == 0:
    sys.exit(0)  # no marketplace skills installed → empty idf/suggestable is fine
sug = idx.get("suggestable_count", 0) or sum(1 for s in idx.get("skills", []) if s.get("suggestable"))
idf = idx.get("idf") or {}
if sug == 0 or not idf:
    print(f"ecc-index integrity: degenerate ({marketplace} marketplace skills, "
          f"suggestable={sug}, idf_terms={len(idf)})", file=sys.stderr)
    sys.exit(1)
PYEOF
    fi
  fi

  # Refresh pattern-detector's dedupe index (skill-index.jsonl) if >24h or missing.
  # Built from the four skill/instinct sources; lets pattern-detector read one
  # file instead of globbing ~1000 SKILL.md per tick. See learning/build-skill-index.sh.
  local sidx="$CLAUDE_HOME/agent-memory/we-forge/skill-index.jsonl"
  local sidx_builder="$CLAUDE_HOME/learning/build-skill-index.sh"
  if [ -f "$sidx_builder" ]; then
    local sidx_mtime now stale=1
    if [ -f "$sidx" ]; then
      if stat -f%m "$sidx" >/dev/null 2>&1; then
        sidx_mtime="$(stat -f%m "$sidx")"
      else
        sidx_mtime="$(stat -c%Y "$sidx" 2>/dev/null)"
      fi
      now="$(date +%s)"
      [ $(( now - ${sidx_mtime:-0} )) -lt 86400 ] && stale=0
    fi
    if [ "$stale" = "1" ]; then
      CLAUDE_HOME="$CLAUDE_HOME" bash "$sidx_builder" >>"$LOG" 2>&1 || _log "skill-index rebuild failed"
    fi
  fi

  # Export learning paths for the we-forge agent invocation below.
  # Without explicit export, the agent inherits launchd/systemd minimal env
  # and falls back to defaults — which breaks when CLAUDE_LEARNING_DATA or
  # CLAUDE_HOME were overridden at install time.
  export CLAUDE_LEARNING_DATA="$DATA_DIR"
  export CLAUDE_LEARNED_SKILLS="${CLAUDE_LEARNED_SKILLS:-$CLAUDE_HOME/skills/learned}"
  export WE_FORGE_HOME="${WE_FORGE_HOME:-$HOME/.we-forge}"

  if _queue_nonempty; then
    _invoke_watch_and_learn
  else
    _log "promotion queue empty; no claude invocation"
  fi

  # Weekly skill-suggest ranking regression check (independent of orchestrator).
  # Catches IDF drift / tokenizer regressions / stale override entries that
  # silently break a previously-working anchor prompt → skill mapping.
  _run_skill_regressions_if_due

  _log "tick end"
}

# Run we-forgectl skill-regressions when last run > 7 days ago (or never).
# On failure, send a Telegram alert (best-effort, mirroring notifier agent
# pattern). The marker is touched on every run regardless of rc so a
# persistent failure doesn't generate weekly spam — one alert per cycle.
_run_skill_regressions_if_due() {
  local marker="${WE_FORGE_HOME:-$HOME/.we-forge}/.last-skill-regressions"
  local cooldown_seconds=604800   # 7 days
  local now last_mtime should_run=0
  now="$(date +%s)"
  if [ ! -f "$marker" ]; then
    should_run=1
  else
    if stat -f%m "$marker" >/dev/null 2>&1; then
      last_mtime="$(stat -f%m "$marker")"
    else
      last_mtime="$(stat -c%Y "$marker" 2>/dev/null)"
    fi
    [ $(( now - ${last_mtime:-0} )) -ge "$cooldown_seconds" ] && should_run=1
  fi
  [ "$should_run" != "1" ] && return 0

  local wfctl="$HOME/.local/bin/we-forgectl"
  if [ ! -x "$wfctl" ]; then
    _log "skill-regressions: we-forgectl binary not at $wfctl — skipping"
    return 0
  fi

  local out rc
  out="$("$wfctl" skill-regressions 2>&1)"
  rc=$?
  touch "$marker" 2>/dev/null || true

  if [ "$rc" = "0" ]; then
    _log "skill-regressions: pass (next check in ~7d)"
    return 0
  fi

  _log "skill-regressions: FAIL rc=$rc — sending telegram alert"

  # Telegram dispatch (mirrors agents/notifier.md pattern).
  local cfg="$HOME/.we-forge/config.json"
  if [ ! -f "$cfg" ]; then
    _log "skill-regressions: config missing — alert skipped"
    return 0
  fi
  local enabled token chat
  enabled=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("telegram_enabled") is True)' "$cfg" 2>/dev/null)
  token=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("telegram_token",""))' "$cfg" 2>/dev/null)
  chat=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("telegram_chat_id",""))' "$cfg" 2>/dev/null)
  if [ "$enabled" != "True" ] || [ -z "$token" ] || [ -z "$chat" ]; then
    _log "skill-regressions: telegram disabled — alert logged but not sent"
    return 0
  fi

  # Truncate to fit Telegram's 4096-char message cap (tail keeps the FAIL rows).
  local body
  body="🚨 we-forge skill-regressions FAIL (rc=$rc)
──────────────────────────────
$(printf '%s\n' "$out" | tail -40)

(next check in ~7d; see /Users/.../we-forge/learning/skill-suggest-regressions.json
 or run: we-forgectl skill-regressions --verbose)"

  if curl -fsS --max-time 15 \
       --data-urlencode "chat_id=$chat" \
       --data-urlencode "text=$body" \
       "https://api.telegram.org/bot$token/sendMessage" >/dev/null 2>&1; then
    _log "skill-regressions: telegram alert sent"
  else
    _log "skill-regressions: telegram POST failed"
  fi
}

_main
