#!/usr/bin/env bash
# build-skill-index.sh — pre-build a flat dedupe index for pattern-detector.
#
# pattern-detector must avoid re-creating a skill that already exists in any of
# four places. Globbing + reading ~1000 SKILL.md files every tick is expensive
# for a haiku-class agent. Instead, this script (run at install + refreshed
# every 24h by tick.sh) walks the four sources once and writes one JSONL line
# per skill/instinct:
#
#   {"source":"learned|marketplace|instinct|evolved","slug":"...","name":"...","desc_head":"..."}
#
# `desc_head` is the first 160 chars of `description` (SKILL.md) or `trigger`
# (instinct YAML) — enough signal for substring-overlap dedupe.
#
# Relationship to build_ecc_index.py / ecc-index.json: that index serves
# `we-forgectl skill-suggest` (BM25-lite matching over marketplace skills only).
# This index serves pattern-detector's dedupe (all four sources, simpler shape).
# They're intentionally separate consumers; do not merge.
#
# Output (override with SKILL_INDEX_OUT):
#   $CLAUDE_HOME/agent-memory/we-forge/skill-index.jsonl
#
# Exit 0 always (a missing source is normal, not an error).

set -u

CLAUDE_HOME="${CLAUDE_HOME:-$HOME/.claude}"
OUT="${SKILL_INDEX_OUT:-$CLAUDE_HOME/agent-memory/we-forge/skill-index.jsonl}"

mkdir -p "$(dirname "$OUT")" 2>/dev/null || true
TMP="$OUT.tmp.$$"

CLAUDE_HOME="$CLAUDE_HOME" python3 - "$TMP" <<'PY'
import os, sys, re, json, glob

claude_home = os.environ["CLAUDE_HOME"]
out_path = sys.argv[1]

def frontmatter(text):
    m = re.search(r'\A---\s*\n(.*?)\n---\s*\n', text, re.S)
    return m.group(1) if m else text  # YAML files: whole file is "frontmatter"

def getval(fm, key):
    m = re.search(r'^[ \t]*' + re.escape(key) + r'[ \t]*:[ \t]*(.*?)[ \t]*$', fm, re.M)
    if not m:
        return ""
    v = m.group(1).strip()
    if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
        v = v[1:-1]
    return v

def read(path):
    try:
        with open(path, "r", errors="replace") as f:
            return f.read()
    except OSError:
        return None

rows = []
seen = set()  # (source, slug) — dedupe within the index itself

def add(source, slug, name, desc):
    key = (source, slug)
    if not slug or key in seen:
        return
    seen.add(key)
    rows.append({"source": source, "slug": slug,
                 "name": name or slug, "desc_head": (desc or "")[:160]})

# 1. we-forge previous learnings
for p in glob.glob(os.path.join(claude_home, "skills/learned/*/SKILL.md")):
    slug = os.path.basename(os.path.dirname(p))
    if slug in ("pending",):
        continue
    t = read(p)
    if t is None:
        continue
    fm = frontmatter(t)
    add("learned", getval(fm, "name") or slug, getval(fm, "name") or slug, getval(fm, "description"))

# 2. ECC marketplace (the big one) — skip the plugins/cache mirror to avoid double-count
for p in glob.glob(os.path.join(claude_home, "plugins/marketplaces/**/SKILL.md"), recursive=True):
    slug = os.path.basename(os.path.dirname(p))
    t = read(p)
    if t is None:
        continue
    fm = frontmatter(t)
    add("marketplace", getval(fm, "name") or slug, getval(fm, "name") or slug, getval(fm, "description"))

# 3. ECC project-scoped instincts (YAML: id + trigger)
for p in glob.glob(os.path.join(claude_home, "homunculus/projects/*/instincts/personal/*.yaml")):
    slug = os.path.splitext(os.path.basename(p))[0]
    t = read(p)
    if t is None:
        continue
    fm = frontmatter(t)
    iid = getval(fm, "id") or slug
    add("instinct", iid, iid, getval(fm, "trigger"))

# 4. ECC evolved skills (project-scoped and global)
for pat in ("homunculus/projects/*/evolved/skills/*/SKILL.md",
            "homunculus/evolved/skills/*/SKILL.md"):
    for p in glob.glob(os.path.join(claude_home, pat)):
        slug = os.path.basename(os.path.dirname(p))
        t = read(p)
        if t is None:
            continue
        fm = frontmatter(t)
        add("evolved", getval(fm, "name") or slug, getval(fm, "name") or slug, getval(fm, "description"))

with open(out_path, "w") as f:
    # leading metadata line so consumers can see freshness
    import datetime
    f.write(json.dumps({"_meta": True,
                        "built_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                        "counts": {s: sum(1 for r in rows if r["source"] == s)
                                   for s in ("learned", "marketplace", "instinct", "evolved")},
                        "total": len(rows)}, ensure_ascii=False) + "\n")
    for r in rows:
        f.write(json.dumps(r, ensure_ascii=False) + "\n")

print(f"skill-index: {len(rows)} entries → {out_path}", file=sys.stderr)
PY
rc=$?

if [ "$rc" -eq 0 ] && [ -s "$TMP" ]; then
  mv -f "$TMP" "$OUT"
else
  rm -f "$TMP" 2>/dev/null || true
  echo "build-skill-index: build failed (rc=$rc); kept previous index" >&2
fi
exit 0
