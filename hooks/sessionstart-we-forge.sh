#!/usr/bin/env bash
# sessionstart-we-forge.sh — print live we-forge state at session start
#
# Registered as a SessionStart hook in ~/.claude/settings.json. Claude Code
# pipes this script's stdout into the model's context before the first user
# turn, so every session begins with awareness of:
#   - daemon status (running / stopped / not-installed)
#   - current cadence + next aligned tick
#   - top ECC marketplace skills leveraged (ROI signal)
#   - recent ledger activity (PASS / ECC_MATCH / DROP totals last 24h)
#
# Always exits 0 so a malformed environment doesn't break the session.

set +e

WF=~/.local/bin/we-forgectl
LEDGER=~/.claude/learning/data/ledger.jsonl
TRACE=~/.we-forge/ecc-trace.jsonl

# Skip silently if not installed.
[ -x "$WF" ] || exit 0

cat <<'HEADER'
================================================================
we-forge live status (auto-loaded by SessionStart hook)
================================================================
HEADER

# 1. Service status + cadence
"$WF" status 2>/dev/null | head -8

echo ""

# 2. ECC marketplace skill usage (top 5)
if [ -s "$TRACE" ]; then
    echo "ECC skill usage (top 5, all-time):"
    python3 -c "
import json, sys
from collections import Counter
c = Counter()
with open('$TRACE') as f:
    for ln in f:
        try:
            c[json.loads(ln).get('skill','?')] += 1
        except: pass
for skill, n in c.most_common(5):
    print(f'  {n:>4}  {skill}')
" 2>/dev/null
    echo ""
fi

# 3. Recent ledger summary (last 24h)
if [ -s "$LEDGER" ]; then
    python3 -c "
import json, datetime as dt
from collections import Counter
cutoff = dt.datetime.now(dt.timezone.utc) - dt.timedelta(hours=24)
c = Counter()
with open('$LEDGER') as f:
    for ln in f:
        try:
            d = json.loads(ln)
            ts = dt.datetime.fromisoformat(d['ts'].replace('Z','+00:00'))
            if ts >= cutoff:
                c[d.get('decision','?')] += 1
        except: pass
total = sum(c.values())
if total:
    parts = ' '.join(f'{k}={v}' for k,v in sorted(c.items()))
    print(f'ledger (last 24h): processed={total} {parts}')
" 2>/dev/null
fi

cat <<'FOOTER'

quick commands:
  /skill-report          - full report
  we-forgectl status     - service + cadence + skill-suggest hit rate
  we-forgectl ecc-trace --group   - ECC skill histogram
  we-forgectl skill-hits - skill-suggest hit rate detail (24h)

PROTOCOL (skill-suggest era + announce, 2026-04-30):
  Every user prompt → UserPromptSubmit hook auto-injects top-3 ECC skill
  candidates (IDF-weighted, ~480 marketplace skills).
    • Match fits intent → "💡 skill-suggest: <name> 사용합니다." + Skill()
    • Suggestions but no match → "skill-suggest: N 후보, 무관 — 일반 진행"
    • Empty injection → silent skip (off-domain prompts).
  Advisor pre/post no longer mandatory (call Agent(subagent_type="Plan")
  only for multi-file architectural or hard-to-reverse changes).
================================================================
FOOTER

exit 0
