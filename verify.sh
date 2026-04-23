#!/usr/bin/env bash
# verify.sh — post-install self-test for we-forge
#
# Wraps the verify steps documented in README.md into a single script.
# Prints one PASS/FAIL line per check; exits non-zero on first failure.
#
# Usage: ./verify.sh
#
# Run after install.sh completes. Safe to re-run; read-only except for
# tick.sh's own dry-run output.

set -u
# Note: NOT set -e — we want to run all checks and report all failures,
# even if an earlier one fails.

CLAUDE_HOME="${CLAUDE_HOME:-$HOME/.claude}"

PASS_COUNT=0
FAIL_COUNT=0
FAILED_CHECKS=()

_color() {
  if [ -t 1 ]; then printf '%s' "$1"; fi
}
_GREEN="$(_color $'\e[32m')"
_RED="$(_color $'\e[31m')"
_YELLOW="$(_color $'\e[33m')"
_RESET="$(_color $'\e[0m')"

_pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf '  %sPASS%s  %s\n' "$_GREEN" "$_RESET" "$1"
}

_fail() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  FAILED_CHECKS+=("$1")
  printf '  %sFAIL%s  %s\n' "$_RED" "$_RESET" "$1"
  if [ -n "${2:-}" ]; then
    printf '         %s\n' "$2"
  fi
}

_warn() {
  printf '  %sWARN%s  %s\n' "$_YELLOW" "$_RESET" "$1"
}

_section() {
  printf '\n%s\n' "$1"
}

# ---------------------------------------------------------------------------

printf 'we-forge verify (CLAUDE_HOME=%s)\n' "$CLAUDE_HOME"

_section "1. Required tools"
for tool in jq python3 bash; do
  if command -v "$tool" >/dev/null 2>&1; then
    _pass "$tool found ($($tool --version 2>&1 | head -1))"
  else
    _fail "$tool not found" "install via your package manager (brew/apt/scoop)"
  fi
done

_section "2. Installed files"
for f in \
  "$CLAUDE_HOME/learning/tick.sh" \
  "$CLAUDE_HOME/learning/redact.sh" \
  "$CLAUDE_HOME/learning/normalize.py" \
  "$CLAUDE_HOME/hooks/stop-telemetry.sh" \
  "$CLAUDE_HOME/agents/we-forge.md" \
  "$CLAUDE_HOME/agents/pattern-detector.md" \
  "$CLAUDE_HOME/agents/skill-synthesizer.md" \
  "$CLAUDE_HOME/agents/quality-auditor.md" \
  "$CLAUDE_HOME/agents/monitor-sentinel.md" \
  "$CLAUDE_HOME/commands/skill-report.md" \
  "$CLAUDE_HOME/commands/watch-and-learn.md" \
  ; do
  if [ -f "$f" ]; then
    _pass "$f"
  else
    _fail "$f missing" "did install.sh run successfully?"
  fi
done

_section "3. Data directory"
if [ -d "$CLAUDE_HOME/learning/data" ]; then
  _pass "$CLAUDE_HOME/learning/data exists"
else
  _fail "$CLAUDE_HOME/learning/data missing" "re-run install.sh"
fi

_section "4. Redaction self-test"
if [ -x "$CLAUDE_HOME/learning/redact.sh" ]; then
  if "$CLAUDE_HOME/learning/redact.sh" --self-test >/dev/null 2>&1; then
    _pass "redact.sh --self-test passes"
  else
    _fail "redact.sh --self-test failed" \
      "run: bash $CLAUDE_HOME/learning/redact.sh --self-test  (to see output)"
  fi
else
  _fail "redact.sh not executable"
fi

_section "5. Stop-hook smoke test"
if [ -x "$CLAUDE_HOME/hooks/stop-telemetry.sh" ]; then
  _STOP_INPUT='{"session_id":"verify-test","transcript_path":"/dev/null","stop_hook_active":false,"cwd":"/tmp"}'
  if printf '%s' "$_STOP_INPUT" | "$CLAUDE_HOME/hooks/stop-telemetry.sh" >/dev/null 2>&1; then
    _pass "stop-telemetry.sh exits 0 on synthetic input"
  else
    _fail "stop-telemetry.sh failed" "stop hooks must always exit 0"
  fi
else
  _fail "stop-telemetry.sh not executable"
fi

_section "6. tick.sh dry-run"
if [ -x "$CLAUDE_HOME/learning/tick.sh" ]; then
  if CLAUDE_DRY_RUN=1 "$CLAUDE_HOME/learning/tick.sh" >/dev/null 2>&1; then
    _pass "tick.sh dry-run completes"
    if [ -f "$CLAUDE_HOME/learning/data/tick.log" ]; then
      _last="$(tail -n 1 "$CLAUDE_HOME/learning/data/tick.log" 2>/dev/null)"
      printf '         last log line: %s\n' "${_last:-(empty)}"
    fi
  else
    _fail "tick.sh dry-run failed" "see $CLAUDE_HOME/learning/data/tick.log"
  fi
else
  _fail "tick.sh not executable"
fi

_section "7. settings.json hook integration"
if [ -f "$CLAUDE_HOME/settings.json" ]; then
  if jq -e '.hooks.Stop[]?.hooks[]?.command' "$CLAUDE_HOME/settings.json" \
       2>/dev/null | grep -q "stop-telemetry"; then
    _pass "Stop hook registered in settings.json"
  else
    _fail "Stop hook not found in settings.json" \
      "re-run install.sh to merge the snippet"
  fi
else
  _fail "$CLAUDE_HOME/settings.json missing"
fi

_section "8. Scheduler (informational)"
case "$(uname -s)" in
  Darwin)
    if launchctl list 2>/dev/null | grep -q "we-forge"; then
      _pass "launchd job loaded"
    else
      _warn "launchd job not loaded — see launchd/com.we-forge-tick.plist.template"
    fi
    ;;
  Linux)
    if systemctl --user list-timers 2>/dev/null | grep -q "we-forge"; then
      _pass "systemd user timer active"
    elif crontab -l 2>/dev/null | grep -q "tick.sh"; then
      _pass "cron entry installed"
    else
      _warn "no scheduler found — paste crontab.example into 'crontab -e'"
      _warn "or install systemd templates from systemd/"
    fi
    ;;
  *)
    _warn "scheduler check skipped on $(uname -s)"
    ;;
esac

# ---------------------------------------------------------------------------

_section "Summary"
printf '  passed: %d\n' "$PASS_COUNT"
printf '  failed: %d\n' "$FAIL_COUNT"
if [ "$FAIL_COUNT" -gt 0 ]; then
  printf '\n%sFailed checks:%s\n' "$_RED" "$_RESET"
  for f in "${FAILED_CHECKS[@]}"; do
    printf '  - %s\n' "$f"
  done
  exit 1
fi
printf '\n%sAll checks passed.%s\n' "$_GREEN" "$_RESET"
