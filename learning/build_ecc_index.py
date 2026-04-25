#!/usr/bin/env python3
"""build_ecc_index.py — pre-build a keyword index of ECC marketplace skills.

Output file (~/.we-forge/ecc-index.json by default; override via WE_FORGE_HOME):

    {
      "built_at":  "2026-04-26T12:00:00Z",
      "skill_count": 485,
      "skills": [
        {
          "slug":        "git-workflow",
          "name":        "git-workflow",
          "description": "Branch + commit hygiene for everyday git ops...",
          "tokens":      ["branch","commit","hygiene","everyday"],
          "source":      "marketplace",
          "path":        "~/.claude/plugins/marketplaces/.../SKILL.md"
        },
        ...
      ]
    }

Sources scanned (in priority order):

  1. ~/.claude/plugins/marketplaces/**/SKILL.md     → source="marketplace"
  2. ~/.claude/skills/learned/*/SKILL.md             → source="learned"
  3. ~/.claude/homunculus/**/evolved/skills/*/SKILL.md → source="evolved"
  4. ~/.claude/homunculus/projects/*/instincts/personal/*.yaml → source="instinct"

The pattern-detector agent reads this index instead of re-scanning ~1000
files every tick — turns dedupe into an O(n) hash lookup with consistent
keyword matching across runs.

Skipped: ~/.claude/plugins/cache/** (duplicates marketplaces).
"""
from __future__ import annotations

import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

# Stop-list kept in sync with agents/pattern-detector.md scoring rules.
_STOPWORDS = {
    "the", "and", "for", "with", "this", "that", "from", "into",
    "when", "where", "which", "after", "before",
    "are", "or", "of", "to", "in", "on", "at",
    "be", "by", "as", "it", "if", "you", "your", "use", "used",
}

_TOKEN_RE = re.compile(r"[A-Za-z][A-Za-z0-9_-]{2,}")


def _tokenize(text: str) -> list[str]:
    """Lowercase content tokens, length >= 4, not in stop-list, deduped."""
    out: list[str] = []
    seen: set[str] = set()
    for m in _TOKEN_RE.finditer(text or ""):
        t = m.group(0).lower()
        if len(t) < 4:
            continue
        if t in _STOPWORDS:
            continue
        if t in seen:
            continue
        seen.add(t)
        out.append(t)
    return out


def _parse_frontmatter(text: str) -> dict:
    """Minimal YAML frontmatter parser — name/description/id/trigger only.

    Avoids a PyYAML dependency. Only reads top-level scalar string fields.
    """
    if not text.startswith("---"):
        return {}
    end = text.find("\n---", 3)
    if end < 0:
        return {}
    block = text[3:end]
    out: dict = {}
    for line in block.splitlines():
        m = re.match(r"^([A-Za-z_][A-Za-z0-9_-]*)\s*:\s*(.*)$", line)
        if not m:
            continue
        key, val = m.group(1), m.group(2).strip()
        if val.startswith('"') and val.endswith('"') and len(val) >= 2:
            val = val[1:-1]
        elif val.startswith("'") and val.endswith("'") and len(val) >= 2:
            val = val[1:-1]
        out[key] = val
    return out


def _index_skill_md(path: Path, source: str) -> dict | None:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    fm = _parse_frontmatter(text)
    name = fm.get("name") or path.parent.name
    description = fm.get("description", "")
    if not name:
        return None
    return {
        "slug": (name or path.parent.name).lower(),
        "name": name,
        "description": description[:400],
        "tokens": _tokenize(name + " " + description)[:30],
        "source": source,
        "path": str(path),
    }


def _index_instinct_yaml(path: Path) -> dict | None:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    fm: dict = {}
    for line in text.splitlines():
        m = re.match(r"^([A-Za-z_][A-Za-z0-9_-]*)\s*:\s*(.*)$", line)
        if m:
            v = m.group(2).strip().strip('"').strip("'")
            fm[m.group(1)] = v
    name = fm.get("id") or path.stem
    trigger = fm.get("trigger", "")
    return {
        "slug": name.lower(),
        "name": name,
        "description": trigger[:400],
        "tokens": _tokenize(name + " " + trigger)[:30],
        "source": "instinct",
        "path": str(path),
    }


def main(argv: list[str]) -> int:
    home = Path.home()
    we_forge_home = Path(os.environ.get("WE_FORGE_HOME", str(home / ".we-forge")))
    we_forge_home.mkdir(parents=True, exist_ok=True)
    out_path = we_forge_home / "ecc-index.json"

    skills: list[dict] = []

    marketplaces = home / ".claude" / "plugins" / "marketplaces"
    if marketplaces.exists():
        for p in marketplaces.rglob("SKILL.md"):
            entry = _index_skill_md(p, "marketplace")
            if entry:
                skills.append(entry)

    learned = home / ".claude" / "skills" / "learned"
    if learned.exists():
        for p in learned.glob("*/SKILL.md"):
            entry = _index_skill_md(p, "learned")
            if entry:
                skills.append(entry)

    homunc = home / ".claude" / "homunculus"
    if homunc.exists():
        for p in homunc.rglob("evolved/skills/*/SKILL.md"):
            entry = _index_skill_md(p, "evolved")
            if entry:
                skills.append(entry)
        for p in homunc.rglob("instincts/personal/*.yaml"):
            entry = _index_instinct_yaml(p)
            if entry:
                skills.append(entry)

    payload = {
        "built_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "skill_count": len(skills),
        "skills": skills,
    }

    out_path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    print(f"build_ecc_index: wrote {out_path} (skill_count={len(skills)})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
