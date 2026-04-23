#!/usr/bin/env bash
# redact.sh — shared secret filter. Source this file to get `redact_line`.
#
# redact_line reads one line on stdin and exits:
#   0  KEEP   — no secret detected
#   1  DROP   — secret or high-entropy token detected; caller must drop the line
#
# This is a filter: it does NOT mask. Dropping a whole event is safer than
# emitting a partially-redacted sample that still leaks structure.
#
# Callers (by design):
#   - hooks/stop-telemetry.sh        (sources this file)
#   - learning/tick.sh               (sources this file)
#   - agents/quality-auditor.md      (invokes via `bash redact.sh` on SKILL.md body)
#   - install.sh --test              (invokes `bash redact.sh --self-test`)
#
# Self-test:  bash redact.sh --self-test

set -u

redact_line() {
  local line
  if ! IFS= read -r line; then
    return 0
  fi

  # Common secret-key name patterns followed by = or : and a non-empty value.
  if printf '%s' "$line" | LC_ALL=C grep -Eiq '(api[_-]?key|passwd|password|secret|token|bearer|authorization)[[:space:]]*[:=][[:space:]]*[^[:space:]]+'; then
    return 1
  fi

  # Known env-var names carrying credentials.
  if printf '%s' "$line" | LC_ALL=C grep -Eq '(ANTHROPIC_API_KEY|OPENAI_API_KEY|AWS_SECRET_ACCESS_KEY|AWS_ACCESS_KEY_ID|GITHUB_TOKEN|GH_TOKEN|HF_TOKEN|SLACK_TOKEN|NPM_TOKEN|STRIPE_KEY|STRIPE_SECRET)'; then
    return 1
  fi

  # Well-known credential prefixes.
  if printf '%s' "$line" | LC_ALL=C grep -Eq '(^|[^A-Za-z0-9])(sk-[A-Za-z0-9_-]{16,}|ghp_[A-Za-z0-9]{20,}|ghs_[A-Za-z0-9]{20,}|gho_[A-Za-z0-9]{20,}|xox[bpsa]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16})'; then
    return 1
  fi

  # High-entropy long-token heuristic.
  # Pick the longest [A-Za-z0-9+/=_-]{32,} substring and compute Shannon entropy.
  local ent_token
  ent_token="$(printf '%s' "$line" | LC_ALL=C grep -Eo '[A-Za-z0-9+/=_-]{32,}' | awk '{ if (length($0) > length(longest)) longest=$0 } END { print longest }')"
  if [ -n "${ent_token:-}" ]; then
    local entropy
    entropy="$(printf '%s' "$ent_token" | awk '
      {
        n = length($0)
        if (n == 0) { print 0; exit }
        for (i = 1; i <= n; i++) {
          c = substr($0, i, 1)
          freq[c]++
        }
        h = 0
        for (c in freq) {
          p = freq[c] / n
          h -= p * log(p) / log(2)
        }
        printf "%.4f", h
      }')"
    # Threshold 4.0: typical base64/hex tokens score > 4.5; English prose scores < 4.0.
    if awk -v e="$entropy" 'BEGIN { exit !(e+0 >= 4.0) }'; then
      return 1
    fi
  fi

  return 0
}

_self_test() {
  local pass=0 fail=0
  _expect() {
    local expected="$1" line="$2" got
    if printf '%s\n' "$line" | redact_line >/dev/null 2>&1; then
      got="KEEP"
    else
      got="DROP"
    fi
    if [ "$got" = "$expected" ]; then
      pass=$((pass+1))
      printf '  ok   %-4s  %s\n' "$got" "$line"
    else
      fail=$((fail+1))
      printf '  FAIL expected=%s got=%s  %s\n' "$expected" "$got" "$line"
    fi
  }

  _expect DROP 'ANTHROPIC_API_KEY=sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'
  _expect DROP 'export GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789'
  _expect DROP 'password: hunter2supersecretvalue'
  _expect DROP 'Authorization: Bearer eyJabcdefghijklmnopqrstuvwxyz0123456789abcdef.payload.sig'
  _expect DROP 'api_key = AKIAIOSFODNN7EXAMPLE'
  _expect DROP 'curl -H "x-token: xoxb-1234567890-abcdefghijklmnop"'
  _expect DROP 'echo A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8S9t0'

  _expect KEEP 'git status'
  _expect KEEP 'ls -la ~/projects'
  _expect KEEP 'npm install react'
  _expect KEEP '# user was reviewing the design doc'
  _expect KEEP 'docker compose up -d'
  _expect KEEP 'make build'

  echo
  printf 'redact self-test: %d pass, %d fail\n' "$pass" "$fail"
  [ "$fail" -eq 0 ]
}

if [ "${BASH_SOURCE[0]:-$0}" = "$0" ]; then
  case "${1:-}" in
    --self-test|-t)
      _self_test
      exit $?
      ;;
    --check|-c)
      # Read one line; exit 0 KEEP / 1 DROP. Used by callers that already
      # spawn a subshell per line and only want the verdict.
      redact_line
      exit $?
      ;;
    *)
      # Filter mode: pass lines through, drop secrets silently. Always exits 0.
      while IFS= read -r line; do
        if printf '%s\n' "$line" | redact_line; then
          printf '%s\n' "$line"
        fi
      done
      ;;
  esac
fi
