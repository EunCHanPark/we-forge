#!/usr/bin/env bash
# verify-skill-suggest.sh — manual end-to-end check for the skill-suggest pipeline.
#
# Use when you suspect skill-suggest is returning "no match" for every prompt
# (the same class of bug as the a78c532 → 2026-05-14 regression). Validates:
#   - binaries and hook script are in place
#   - ecc-index.json has the structural shape the Rust matcher requires
#     (root-level `idf` dict, per-skill `suggestable=true`)
#   - the BM25-lite ranker actually returns hits for representative prompts
#   - the UserPromptSubmit hook end-to-end emits a system-reminder block
#
# Exits non-zero on the first hard failure. Soft warnings (e.g. missing hook
# script) print and continue.

set -e

WF=~/.local/bin/we-forgectl
HOOK=~/.claude/hooks/userpromptsubmit-skill-suggest.sh
INDEX=~/.we-forge/ecc-index.json
SUGG_LOG=~/.we-forge/skill-suggestions.jsonl

echo "================================================================"
echo "skill-suggest verification"
echo "================================================================"

echo
echo "[1/5] Binary + script presence"
[ -x "$WF" ]    && echo "  ✓ $WF" || { echo "  ✗ $WF missing"; exit 1; }
[ -f "$INDEX" ] && echo "  ✓ $INDEX" \
                || { echo "  ✗ $INDEX missing — rebuild via: python3 learning/build_ecc_index.py"; exit 1; }
if [ -x "$HOOK" ]; then
  echo "  ✓ $HOOK"
else
  echo "  ⚠ $HOOK missing (hook test [4/5] will be skipped)"
fi

echo
echo "[2/5] Index integrity (idf + suggestable shape required by Rust matcher)"
python3 - "$INDEX" <<'PYEOF'
import json, sys
idx = json.load(open(sys.argv[1], encoding="utf-8"))
total = len(idx.get("skills", []))
marketplace = sum(1 for s in idx.get("skills", []) if s.get("source") == "marketplace")
sug = idx.get("suggestable_count", 0) or sum(1 for s in idx.get("skills", []) if s.get("suggestable"))
idf = idx.get("idf") or {}
print(f"  skills={total}  marketplace={marketplace}  suggestable={sug}  idf_terms={len(idf)}")
if marketplace == 0:
    print("  ⚠ no marketplace skills installed — suggestions will be empty by design")
    sys.exit(0)
if sug == 0:
    print("  ✗ suggestable_count=0 — per-skill `suggestable` flag missing", file=sys.stderr); sys.exit(1)
if not idf:
    print("  ✗ idf empty — IDF computation missing", file=sys.stderr); sys.exit(1)
print("  ✓ index shape OK")
PYEOF

echo
echo "[3/5] Subcommand availability"
"$WF" skill-suggest --help >/dev/null 2>&1 \
  && echo "  ✓ we-forgectl skill-suggest" \
  || { echo "  ✗ skill-suggest not registered (re-deploy: cp scripts/we-forgectl ~/.local/bin/we-forgectl)"; exit 1; }
"$WF" skill-hits --help >/dev/null 2>&1 \
  && echo "  ✓ we-forgectl skill-hits" \
  || { echo "  ✗ skill-hits not registered"; exit 1; }

echo
echo "[4/5] Direct matcher (5 sample prompts, expect ≥4 hits)"
SAMPLES=(
  "PostgreSQL 인덱스 설계와 쿼리 최적화"
  "Python pytest로 TDD 테스트 추가"
  "Docker 컨테이너 설정과 docker-compose"
  "Rust 비동기 동시성 코드 검토"
  "오늘 날씨 어때"
)
HITS=0
for p in "${SAMPLES[@]}"; do
  RES=$("$WF" skill-suggest --top 1 "$p" 2>&1)
  if echo "$RES" | grep -qE "^\s+1\."; then
    TOP=$(echo "$RES" | grep -E "^\s+1\." | head -1 | awk '{print $2}')
    echo "  '$p' → $TOP"
    HITS=$((HITS+1))
  else
    echo "  '$p' → (no match)"
  fi
done
if [ "$HITS" -ge 4 ]; then
  echo "  ✓ matcher recall OK ($HITS/5)"
else
  echo "  ✗ matcher recall low ($HITS/5) — index may be degenerate"
  exit 1
fi

echo
echo "[5/5] Hook end-to-end (simulated UserPromptSubmit payload)"
if [ -x "$HOOK" ]; then
  PAYLOAD='{"session_id":"verify-test","transcript_path":"/tmp/v","cwd":"/tmp","hook_event_name":"UserPromptSubmit","prompt":"Add JWT 인증 to PostgreSQL backend service with rate limiting middleware"}'
  OUT=$(echo "$PAYLOAD" | "$HOOK")
  if echo "$OUT" | grep -q "<system-reminder>"; then
    echo "  ✓ hook emits system-reminder block ($(echo "$OUT" | wc -l | tr -d ' ') lines)"
    echo "  ── preview ──"
    echo "$OUT" | head -8 | sed 's/^/      /'
  else
    echo "  ✗ hook produced no output for a substantive prompt"
    exit 1
  fi
else
  echo "  ⚠ hook missing — skipped"
fi

echo
if [ -f "$SUGG_LOG" ]; then
  N=$(wc -l < "$SUGG_LOG")
  echo "telemetry: $SUGG_LOG ($N entries)"
else
  echo "telemetry: no $SUGG_LOG yet — fires on first --log invocation"
fi

echo
echo "----------------------------------------------------------------"
echo "Verification PASSED."
echo "----------------------------------------------------------------"
