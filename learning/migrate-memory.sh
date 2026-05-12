#!/usr/bin/env bash
# migrate-memory.sh — one-time split of the legacy single-file
# ~/.claude/agent-memory/we-forge/MEMORY.md into the 3-tier layout:
#
#   hot.md       — recent raw decision log (rolling ~14d window at migration time)
#   lessons.md   — curated lessons + an "Archived (pre-migration)" block holding
#                  the older Orchestration Log, User Preferences, and the durable
#                  Orchestration Hints
#   pointers.md  — machine-parseable JSON: blocklist / primitive_re / ecc_seen /
#                  ecc_recs / tick_counter / hwm / dead_skill_candidates
#
# Safety:
#   - backs up MEMORY.md → MEMORY.md.bak.<UTC-timestamp>
#   - renames MEMORY.md → MEMORY.md.legacy (does NOT delete it)
#   - is a no-op if pointers.md already exists (migration already done)
#   - is a no-op if there is no MEMORY.md (fresh install — memory-manager will
#     create empty 3-tier files itself)
#
# Run by install.sh on upgrade, and once by hand during the P1-2 rollout.
# Exit 0 on success or benign no-op; non-zero only on real failure.

set -u

CLAUDE_HOME="${CLAUDE_HOME:-$HOME/.claude}"
MEMDIR="$CLAUDE_HOME/agent-memory/we-forge"
LEGACY="$MEMDIR/MEMORY.md"
POINTERS="$MEMDIR/pointers.md"

mkdir -p "$MEMDIR" 2>/dev/null || true

if [ -f "$POINTERS" ]; then
  echo "migrate-memory: pointers.md already exists — already migrated, nothing to do." >&2
  exit 0
fi
if [ ! -f "$LEGACY" ]; then
  echo "migrate-memory: no MEMORY.md — fresh install, nothing to migrate." >&2
  exit 0
fi

TS="$(date -u +%Y%m%dT%H%M%SZ)"
cp -p "$LEGACY" "$MEMDIR/MEMORY.md.bak.$TS" || { echo "migrate-memory: backup failed" >&2; exit 1; }

MEMDIR="$MEMDIR" python3 - <<'PY'
import os, re, json, datetime, sys

memdir = os.environ["MEMDIR"]
legacy = os.path.join(memdir, "MEMORY.md")
text = open(legacy, "r", errors="replace").read()
lines = text.splitlines()

# Split into sections keyed by "## Header".
sections = {}
order = []
cur = "_preamble"
sections[cur] = []
for ln in lines:
    m = re.match(r'^##\s+(.*)$', ln)
    if m:
        cur = m.group(1).strip()
        if cur not in sections:
            sections[cur] = []
            order.append(cur)
    else:
        sections[cur].append(ln)

def sec(name):
    return sections.get(name, [])

# --- pointers.md (JSON) -------------------------------------------------------
# Primitive Blocklist: lines like  - `^bash-grep-` (comment...)  -> the regex
primitive_re = []
for ln in sec("Primitive Blocklist"):
    m = re.search(r'^\s*-\s*`([^`]+)`', ln)
    if m:
        primitive_re.append(m.group(1))

# Rejected-Pattern Blocklist: bare slug lines  - <slug>
blocklist = []
for ln in sec("Rejected-Pattern Blocklist"):
    m = re.search(r'^\s*-\s*([A-Za-z0-9][\w-]*)\s*$', ln)
    if m:
        blocklist.append(m.group(1))

# ECC Marketplace Recommendations:
#   - <slug>  →  /everything-claude-code:<skill>  (count=<n>, first_seen=<date>...)
ecc_recs = []
ecc_seen = []
for ln in sec("ECC Marketplace Recommendations"):
    m = re.search(r'^\s*-\s*([\w-]+)\s*[-→>]+\s*/everything-claude-code:([\w-]+).*?count=(\d+).*?first_seen=([\d-]+)', ln)
    if m:
        slug, skill, cnt, fs = m.group(1), m.group(2), int(m.group(3)), m.group(4)
        ecc_recs.append({"slug": slug, "ecc_skill": skill, "count": cnt, "first_seen": fs})
        if skill not in ecc_seen:
            ecc_seen.append(skill)

# Orchestration Hints: HWM + the latest tick number (for tick_counter).
hints_text = "\n".join(sec("Orchestration Hints"))
hwm = ""
m = re.search(r'HWM[^`\n]*`([0-9T:\-Z]+)`', hints_text)
if m:
    hwm = m.group(1)
log_text = "\n".join(sec("Orchestration Log"))
tick_nums = [int(x) for x in re.findall(r'tick-(\d+)', log_text)]
tick_counter = max(tick_nums) if tick_nums else 0
# also catch a higher HWM in the log if Hints didn't have one
if not hwm:
    hwms = re.findall(r'HWM→([0-9T:\-Z]+)', log_text)
    if hwms:
        hwm = sorted(hwms)[-1]

pointers = {
    "_meta": {"migrated_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
              "from": "MEMORY.md"},
    "blocklist": blocklist,
    "primitive_re": primitive_re,
    "ecc_seen": ecc_seen,
    "ecc_recs": ecc_recs,
    "tick_counter": tick_counter,
    "hwm": hwm,
    "dead_skill_candidates": [],
}
with open(os.path.join(memdir, "pointers.md"), "w") as f:
    f.write("# we-forge memory — pointers (machine-parseable)\n\n")
    f.write("```json\n")
    f.write(json.dumps(pointers, indent=2, ensure_ascii=False))
    f.write("\n```\n")

# --- hot.md (recent raw decision log) ----------------------------------------
# Keep Orchestration Log entries dated within the last ~14 days; archive the rest.
today = datetime.date.today()
cutoff = today - datetime.timedelta(days=14)
log_lines = [l for l in sec("Orchestration Log") if l.strip()]
def line_date(l):
    m = re.search(r'(\d{4}-\d{2}-\d{2})', l)
    if not m:
        return None
    try:
        return datetime.date.fromisoformat(m.group(1))
    except ValueError:
        return None
hot_lines, archived_log = [], []
for l in log_lines:
    d = line_date(l)
    if d is not None and d >= cutoff:
        hot_lines.append(l)
    else:
        archived_log.append(l)
# Always keep at least the last 12 log lines hot, even if undated.
if len(hot_lines) < 12:
    extra = [l for l in archived_log[-(12 - len(hot_lines)):]]
    archived_log = archived_log[:len(archived_log) - len(extra)]
    hot_lines = extra + hot_lines
# Enforce a ~9 KB cap on hot.md (memory-manager's cap is 10 KB; leave headroom):
# move the oldest hot lines back to the archive until the body fits.
HOT_CAP = 9000
def hot_bytes(ls):
    return sum(len(l) + 1 for l in ls)
while len(hot_lines) > 12 and hot_bytes(hot_lines) > HOT_CAP:
    archived_log.append(hot_lines.pop(0))

with open(os.path.join(memdir, "hot.md"), "w") as f:
    f.write("# we-forge memory — hot (recent raw decision log, rolling ~7d window)\n")
    f.write("<!-- format: <slug> <PASS|REVISE|REJECT|ECC_MATCH|DROP> <date> [note], or <!-- tick-N ... --> -->\n")
    f.write("<!-- memory-manager rolls entries older than 7d into lessons.md on each `record` call -->\n\n")
    for l in hot_lines:
        f.write(l + "\n")

# --- lessons.md (curated + archived) -----------------------------------------
def block(name):
    body = "\n".join(sec(name)).strip()
    return body
with open(os.path.join(memdir, "lessons.md"), "w") as f:
    f.write("# we-forge memory — lessons (compressed, curated)\n\n")
    f.write("## Lessons\n")
    f.write("<!-- one line per durable, non-obvious pattern. memory-manager appends rollup -->\n")
    f.write("<!-- summaries here; curate by hand or via /skill-report follow-ups -->\n")
    # seed a couple distilled from the legacy rollups (safe, factual)
    f.write("- The promotion queue has converged on a stable canonical slug-set since ~tick-75: ~70-80 entries/tick, all primitive-blocklist DROP or known ECC_MATCH; zero novel-compositional promotions expected until genuinely new multi-tool workflows appear.\n")
    f.write("- ECC_MATCH skews to `dmux-workflows` (tmux/codex family, 6 slugs) and `agentic-engineering` (agent-opaque); these are the only marketplace skills the user's command stream keeps shadowing.\n")
    f.write("- Shell single-tool primitives dominate DROP (~90% of each tick's verdicts); the primitive blocklist regex set is the main throughput lever.\n\n")
    f.write("## Durable orchestration hints (from legacy MEMORY.md)\n")
    b = block("Orchestration Hints")
    f.write((b if b else "<!-- (none) -->") + "\n\n")
    up = block("User Preferences")
    f.write("## User Preferences (from legacy MEMORY.md)\n")
    f.write((up if up else "<!-- (none recorded) -->") + "\n\n")
    f.write("## Archived Orchestration Log (pre-migration)\n")
    f.write("<!-- frozen at migration; not updated. See hot.md for the live tail. -->\n")
    if archived_log:
        for l in archived_log:
            f.write(l + "\n")
    else:
        f.write("<!-- (everything was within the hot window) -->\n")

print("migrate-memory: wrote pointers.md, hot.md, lessons.md", file=sys.stderr)
print(json.dumps({"primitive_re": len(primitive_re), "ecc_recs": len(ecc_recs),
                  "blocklist": len(blocklist), "tick_counter": tick_counter, "hwm": hwm}), file=sys.stderr)
PY
rc=$?

if [ "$rc" -ne 0 ]; then
  echo "migrate-memory: python migration failed (rc=$rc); legacy MEMORY.md untouched" >&2
  rm -f "$POINTERS" 2>/dev/null || true
  exit 1
fi

# Sanity: all three files non-empty.
for f in pointers.md hot.md lessons.md; do
  [ -s "$MEMDIR/$f" ] || { echo "migrate-memory: $f is empty after migration — aborting, legacy untouched" >&2; exit 1; }
done

mv -f "$LEGACY" "$MEMDIR/MEMORY.md.legacy"
echo "migrate-memory: done. legacy → $MEMDIR/MEMORY.md.legacy (backup: MEMORY.md.bak.$TS)" >&2
exit 0
