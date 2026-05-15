#!/usr/bin/env bash
# we-forge daemon dead-man supervisor
#
# Why: launchd KeepAlive=true 가 daemon process 죽었을 때만 재시작 처리. process 가
# *살아있지만 무한 loop / hung* 인 경우는 launchd 가 인지 못함 (ECC security guide
# §"Kill Switches" 가 정확히 지적한 갭).
#
# What: daemon.log 의 `tick begin` / `tick end` timestamp 를 분석해 hung 감지하면
# daemon process group SIGTERM → SIGKILL. launchd 가 즉시 재시작.
#
# Cases detected:
#   1) tick hung — `tick begin` 후 TICK_HUNG_THRESHOLD(30분) 안에 `tick end` 없음
#   2) tick missed — last `tick begin` 후 NO_TICK_THRESHOLD(6.5h) 경과 — 새 tick 미시작
#
# Logs:
#   - supervisor own log: ~/Library/Logs/we-forge/supervisor.log
#   - daemon log (read-only): ~/Library/Logs/we-forge/daemon.log

set -euo pipefail

DAEMON_LOG="${HOME}/Library/Logs/we-forge/daemon.log"
SUPERVISOR_LOG="${HOME}/Library/Logs/we-forge/supervisor.log"
TICK_HUNG_THRESHOLD=1800    # 30 min — tick during > this = hung
INTERVAL_SEC=21600          # 6h aligned interval
NO_TICK_THRESHOLD=23400     # 6.5h — no new tick begin = daemon stalled (interval + 30min margin)

mkdir -p "$(dirname "$SUPERVISOR_LOG")"

now_iso() { date -u "+%Y-%m-%dT%H:%M:%SZ"; }
log()     { echo "[$(now_iso)] $*" >> "$SUPERVISOR_LOG"; }

if [ ! -f "$DAEMON_LOG" ]; then
  log "daemon.log not found at $DAEMON_LOG — nothing to supervise"
  exit 0
fi

# Most recent "tick begin"
last_begin_line=$(grep "tick begin" "$DAEMON_LOG" 2>/dev/null | tail -1 || true)
if [ -z "$last_begin_line" ]; then
  log "no tick begin yet in daemon.log"
  exit 0
fi

last_begin_ts=$(echo "$last_begin_line" | grep -oE '\[[0-9-]+T[0-9:]+Z\]' | tr -d '[]')
last_begin_epoch=$(date -j -u -f "%Y-%m-%dT%H:%M:%SZ" "$last_begin_ts" "+%s" 2>/dev/null || echo 0)
now_epoch=$(date -u "+%s")
age_sec=$((now_epoch - last_begin_epoch))

# Did a matching "tick end" arrive after the last begin?
tick_completed=$(awk -v ts="[$last_begin_ts]" '$0 ~ "tick end" && $1 >= ts' "$DAEMON_LOG" | tail -1 || true)

verdict="ok"
reason=""

if [ -z "$tick_completed" ] && [ "$age_sec" -gt "$TICK_HUNG_THRESHOLD" ]; then
  verdict="hung"
  reason="tick begin at $last_begin_ts, age ${age_sec}s, no tick end yet (threshold ${TICK_HUNG_THRESHOLD}s)"
elif [ -n "$tick_completed" ] && [ "$age_sec" -gt "$NO_TICK_THRESHOLD" ]; then
  # last tick completed but new tick should have started by now
  verdict="missed"
  reason="last tick begin at $last_begin_ts (completed), age ${age_sec}s, no new tick scheduled (threshold ${NO_TICK_THRESHOLD}s)"
fi

if [ "$verdict" = "ok" ]; then
  # quiet success — no log spam every 5min. Log only state changes.
  # Touch a heartbeat file so external tools can confirm supervisor itself is alive.
  date -u "+%Y-%m-%dT%H:%M:%SZ ok" > "${HOME}/.we-forge/supervisor-last-check.txt"
  exit 0
fi

log "DETECTED $verdict — $reason"

# Find daemon process — launchd label match more reliable than command match
daemon_pid=$(pgrep -f "we-forgectl daemon" | head -1 || true)
if [ -z "$daemon_pid" ]; then
  log "  daemon pid not found via pgrep — launchd may already be restarting"
  exit 0
fi

# Get process group (-pgid same as -pid on macOS for launchd children w/ ProcessType Background)
pgid=$(ps -o pgid= -p "$daemon_pid" 2>/dev/null | tr -d ' ' || true)
[ -z "$pgid" ] && pgid="$daemon_pid"

log "  killing daemon pid=$daemon_pid pgid=$pgid (SIGTERM then SIGKILL)"
kill -TERM "-$pgid" 2>/dev/null || true
sleep 5

# Check if still alive
if kill -0 "$daemon_pid" 2>/dev/null; then
  log "  SIGTERM ignored, escalating to SIGKILL"
  kill -KILL "-$pgid" 2>/dev/null || true
  sleep 2
fi

if kill -0 "$daemon_pid" 2>/dev/null; then
  log "  ERROR: daemon pid=$daemon_pid still alive after SIGKILL — manual intervention required"
else
  log "  daemon killed, launchd KeepAlive will restart"
fi

date -u "+%Y-%m-%dT%H:%M:%SZ $verdict killed" > "${HOME}/.we-forge/supervisor-last-check.txt"
