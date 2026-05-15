#!/usr/bin/env python3
"""build_ecc_index.py — pre-build a keyword index of ECC marketplace skills.

Output file (~/.we-forge/ecc-index.json by default; override via WE_FORGE_HOME):

    {
      "built_at":          "2026-04-27T12:00:00Z",
      "skill_count":       485,
      "suggestable_count": 410,
      "idf":               { "token": float, ... },   # BM25-lite IDF per token
      "skills": [
        {
          "slug":            "git-workflow",
          "name":            "git-workflow",
          "namespaced_slug": "everything-claude-code:git-workflow",
          "description":     "Branch + commit hygiene for everyday git ops...",
          "tokens":          ["branch","commit","hygiene","everyday"],
          "source":          "marketplace",
          "suggestable":     true,
          "path":            "~/.claude/plugins/marketplaces/.../SKILL.md"
        },
        ...
      ]
    }

Sources scanned (in priority order):

  1. ~/.claude/plugins/marketplaces/**/SKILL.md     → source="marketplace"
  2. ~/.claude/skills/learned/*/SKILL.md             → source="learned"
  3. ~/.claude/homunculus/**/evolved/skills/*/SKILL.md → source="evolved"
  4. ~/.claude/homunculus/projects/*/instincts/personal/*.yaml → source="instinct"

Two consumer paths:
  - pattern-detector: reads `tokens` for dedupe.
  - skill-suggest:    reads `idf` + `tokens` + `suggestable` for prompt
                      matching at UserPromptSubmit hook time.

`suggestable=false` for operational skills that should not be auto-suggested
(dashboards, session ops, schedulers — they don't solve user problems).

Skipped: ~/.claude/plugins/cache/** (duplicates marketplaces) and any
non-canonical localized/IDE-specific copies (docs/zh-CN/, .agents/, .cursor/,
.kiro/, examples/) — a heavily-localized skill is otherwise over-counted and
its IDF is skewed.
"""
from __future__ import annotations

import json
import math
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

# Per-skill token cap for the index. Higher = more match surface for
# skill-suggest. Bumped from 30 → 40 on 2026-05-15 because the new ko↔en
# synonym expansion in `_tokenize()` was pushing 29% of marketplace skills
# (73/254) into cap-saturation, displacing meaningful English tokens with
# their Korean equivalents. Index size impact: ~25 KB, negligible.
_TOKEN_CAP = 40

# Hangul-syllable run (2+ syllables). Single-syllable Korean tokens (e.g. "팟",
# "앱") are almost always particles or fragments; words of meaning are ≥2.
_HANGUL_RE = re.compile(r"[가-힣]{2,}")

# Korean ↔ English synonym map used at index-build time AND at query-time
# (Rust matcher mirrors this list). Keys are Korean tokens that appear in
# real user prompts; values are English equivalents that already appear in
# ECC skill descriptions. We expand BOTH ways:
#   - in a SKILL description containing "deploy", we also stash "배포" as a
#     token, so a Korean prompt can match it without any English overlap.
#   - in a prompt containing "배포", the Rust query expander adds "deploy".
# Keep entries small and high-precision; ambiguous words cause false matches.
_KO_EN_SYNONYMS: dict[str, list[str]] = {
    "검증": ["verify", "verification"],
    "검사": ["check", "inspection"],
    "확인": ["verify", "check"],
    "배포": ["deploy", "deployment", "release"],
    "릴리즈": ["release"],
    "회귀": ["regression"],
    "엔드포인트": ["endpoint"],
    "정적": ["static"],
    "자산": ["asset"],
    "모니터": ["monitor", "monitoring"],
    "모니터링": ["monitor", "monitoring"],
    "테스트": ["test", "testing"],
    "보안": ["security", "secure"],
    "성능": ["performance"],
    "리뷰": ["review"],
    "코드리뷰": ["review", "code"],
    "쿼리": ["query"],
    "스키마": ["schema"],
    "마이그레이션": ["migration"],
    "개발": ["development", "dev"],
    "프롬프트": ["prompt"],
    "에이전트": ["agent"],
    "스킬": ["skill"],
    "워크플로": ["workflow"],
    "워크플로우": ["workflow"],
    "디버그": ["debug", "debugging"],
    "디버깅": ["debug", "debugging"],
    "패턴": ["pattern"],
    "색인": ["index", "indexing"],
    "인덱스": ["index"],
    "로그": ["log", "logging"],
    "리팩토링": ["refactor", "refactoring"],
    "최적화": ["optimization", "optimize"],
    "캐시": ["cache", "caching"],
    "데이터베이스": ["database"],
    "데이터": ["data"],
    "프론트엔드": ["frontend"],
    "백엔드": ["backend"],
    "도커": ["docker"],
    "컨테이너": ["container"],
    "빌드": ["build"],
    "린트": ["lint", "linting"],
    "린터": ["linter"],
    "타입": ["type"],
    "함수": ["function"],
    "컴포넌트": ["component"],
    "라이브러리": ["library"],
    "프레임워크": ["framework"],
    "문서": ["documentation", "docs"],
    "발표": ["presentation", "slide"],
    "슬라이드": ["slide", "presentation"],
    "그래프": ["graph"],
    "노트": ["note"],
    "지식": ["knowledge"],
    "기억": ["memory"],
    "메모리": ["memory"],
    "세션": ["session"],
    "이메일": ["email", "mail"],
    "메일": ["mail", "email"],
    "결제": ["billing", "payment"],
    "청구": ["billing"],
    "환불": ["refund"],
    "고객": ["customer"],
    "재고": ["inventory"],
    "물류": ["logistics", "shipping"],
    "반품": ["return"],
}

# Operational skills whose slugs should never be auto-suggested by the
# UserPromptSubmit skill-suggest hook. They're commands, not problem-solvers.
#
# Two sets:
#   exact:  full-slug match (e.g. `dashboard`, but NOT `dashboard-builder`)
#   prefix: slug starts with `<pfx>-` (e.g. `ping-` blocks `ping-forge`)
#
# Anything else with `source=marketplace` is suggestable. Skills like
# `dashboard-builder`, `skill-stocktake`, `security-review`, `ai-regression-testing`
# are NOT filtered — those teach how to do things.
_NONSUGGESTABLE_EXACT = {
    "dashboard",
    "skill-report",
    "skill-health",
    "watch-and-learn",
    "sessions",
    "save-session",
    "resume-session",
    "loop",
    "schedule",
    "aside",
    "configure-ecc",
    "init",
    "review",  # built-in /review command, ambiguous noise
    "team-builder",  # interactive picker, operational
    "promote",
    "evolve",
    "prune",
    "projects",
    "instinct-import",
    "instinct-export",
    "instinct-status",
    "agent-sort",  # produces install plans, not problem-solving
}

_NONSUGGESTABLE_PREFIXES = (
    "ping-",   # ping-forge etc.
    "hookify-",  # hookify-list/configure/help — operational
)

# Marketplace checkouts ship the same SKILL.md many times: localized doc copies
# (docs/zh-CN/, docs/ja-JP/, …) and IDE-specific copies (.agents/, .cursor/, .kiro/).
# Index only the canonical English copy — a path with any of these segments above
# the skill dir is a non-canonical duplicate (otherwise a heavily-localized popular
# skill is over-counted and its IDF is skewed).
_NONCANONICAL_SEGMENTS = {"docs", ".agents", ".cursor", ".kiro", "examples"}


def _is_noncanonical(path: Path) -> bool:
    return any(seg in _NONCANONICAL_SEGMENTS for seg in path.parts)


def _is_suggestable(slug: str, source: str) -> bool:
    """Return False for operational/command skills that shouldn't auto-suggest.

    Rule: only `marketplace` source is suggestable, and the slug must not
    appear in the exact-deny set or start with a deny prefix.
    `learned`/`evolved`/`instinct` are user-private and might be experimental —
    keep them out of suggestions until promoted to marketplace.
    """
    if source != "marketplace":
        return False
    s = slug.lower()
    if s in _NONSUGGESTABLE_EXACT:
        return False
    for pfx in _NONSUGGESTABLE_PREFIXES:
        if s.startswith(pfx):
            return False
    return True


def _namespace_from_path(path: Path) -> str:
    """Derive plugin namespace from SKILL.md path.

    For ~/.claude/plugins/marketplaces/<plugin>/skills/<skill>/SKILL.md
    the plugin name is the directory two levels up from `skills/`.
    Returns empty string if not a plugin path (learned/evolved/instinct).
    """
    parts = path.parts
    if "marketplaces" in parts:
        i = parts.index("marketplaces")
        if i + 1 < len(parts):
            return parts[i + 1]
    return ""


def _tokenize(text: str) -> list[str]:
    """Lowercase content tokens, length >= 3, not in stop-list, deduped.

    Length >= 3 to retain technical acronyms (jwt, api, sql, ssh, dns, css,
    ide, sdk, npm, mcp). The stop-list excludes English connectives so noise
    is bounded.

    Compound tokens (hyphen-separated, e.g. `golang-patterns`) are emitted
    BOTH as the original compound (for exact slug matching) AND as their
    individual components (so prompt token `golang` matches a skill whose
    only relevant token is `golang-patterns`).

    Hangul runs (`_HANGUL_RE`, ≥2 syllables) are emitted as their own tokens.
    When a Korean token has an entry in `_KO_EN_SYNONYMS`, the English
    equivalents are also emitted — this lets a Korean-only prompt match a
    skill whose description is English-only (and vice-versa, since the
    indexer runs this same function on every skill description).
    """
    out: list[str] = []
    seen: set[str] = set()

    def _add(t: str, min_len: int = 3) -> None:
        if len(t) < min_len or t in _STOPWORDS or t in seen:
            return
        seen.add(t)
        out.append(t)

    # ASCII alphanumeric tokens (existing behavior)
    for m in _TOKEN_RE.finditer(text or ""):
        t = m.group(0).lower()
        _add(t)
        if "-" in t or "_" in t:
            for part in re.split(r"[-_]", t):
                _add(part)

    # Hangul runs — min 2 syllables (each syllable ≥ 3 bytes in UTF-8)
    for m in _HANGUL_RE.finditer(text or ""):
        ko = m.group(0)
        _add(ko, min_len=2)
        # Synonym expansion: emit English equivalents too
        for eng in _KO_EN_SYNONYMS.get(ko, ()):
            _add(eng)

    # Reverse-direction synonym expansion: if an English token in the result
    # has a Korean key in _KO_EN_SYNONYMS, emit the Korean too. This way a
    # canary-watch description containing "deploy" also gets indexed as "배포".
    # Plural-strip variants are tried because skill descriptions naturally
    # contain "endpoints" / "regressions" / "deploys", but the synonym map
    # keys the singular form.
    en_to_ko: dict[str, list[str]] = {}
    for ko, engs in _KO_EN_SYNONYMS.items():
        for eng in engs:
            en_to_ko.setdefault(eng, []).append(ko)
    for tok in list(out):
        variants = [tok]
        if tok.endswith("ies") and len(tok) > 5:
            variants.append(tok[:-3] + "y")
        elif tok.endswith("es") and len(tok) > 5:
            variants.append(tok[:-2])
        elif tok.endswith("s") and not tok.endswith("ss") and len(tok) > 4:
            variants.append(tok[:-1])
        elif tok.endswith("ing") and len(tok) > 5:
            variants.append(tok[:-3])
        for v in variants:
            for ko in en_to_ko.get(v, ()):
                _add(ko, min_len=2)

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
    slug = (name or path.parent.name).lower()
    plugin = _namespace_from_path(path) if source == "marketplace" else ""
    namespaced = f"{plugin}:{name}" if plugin else name
    return {
        "slug": slug,
        "name": name,
        "namespaced_slug": namespaced,
        "description": description[:400],
        "tokens": _tokenize(name + " " + description)[:_TOKEN_CAP],
        "source": source,
        "suggestable": _is_suggestable(slug, source),
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
    slug = name.lower()
    return {
        "slug": slug,
        "name": name,
        "namespaced_slug": name,  # instincts have no plugin namespace
        "description": trigger[:400],
        "tokens": _tokenize(name + " " + trigger)[:_TOKEN_CAP],
        "source": "instinct",
        "suggestable": False,  # instincts are private/experimental
        "path": str(path),
    }


def _compute_idf(skills: list[dict]) -> dict[str, float]:
    """BM25-style IDF: idf[t] = log((N - df + 0.5) / (df + 0.5) + 1).

    Computed only over `suggestable` skills so common operational tokens
    (e.g. "dashboard", "session") don't deflate IDF for problem-solving skills.
    """
    suggestable = [s for s in skills if s.get("suggestable")]
    n = len(suggestable)
    if n == 0:
        return {}
    df: dict[str, int] = {}
    for s in suggestable:
        for t in set(s.get("tokens", [])):
            df[t] = df.get(t, 0) + 1
    return {
        t: round(math.log((n - d + 0.5) / (d + 0.5) + 1.0), 4)
        for t, d in df.items()
    }


def main(argv: list[str]) -> int:
    home = Path.home()
    we_forge_home = Path(os.environ.get("WE_FORGE_HOME", str(home / ".we-forge")))
    we_forge_home.mkdir(parents=True, exist_ok=True)
    out_path = we_forge_home / "ecc-index.json"

    skills: list[dict] = []
    seen: set[tuple[str, str]] = set()  # (source, slug) — drop duplicate paths

    def _add(entry: dict | None) -> None:
        if not entry:
            return
        key = (entry.get("source", ""), entry.get("slug", ""))
        if not key[1] or key in seen:
            return
        seen.add(key)
        skills.append(entry)

    marketplaces = home / ".claude" / "plugins" / "marketplaces"
    if marketplaces.exists():
        for p in marketplaces.rglob("SKILL.md"):
            if _is_noncanonical(p):
                continue
            _add(_index_skill_md(p, "marketplace"))

    learned = home / ".claude" / "skills" / "learned"
    if learned.exists():
        for p in learned.glob("*/SKILL.md"):
            _add(_index_skill_md(p, "learned"))

    homunc = home / ".claude" / "homunculus"
    if homunc.exists():
        for p in homunc.rglob("evolved/skills/*/SKILL.md"):
            _add(_index_skill_md(p, "evolved"))
        for p in homunc.rglob("instincts/personal/*.yaml"):
            _add(_index_instinct_yaml(p))

    idf = _compute_idf(skills)
    suggestable_count = sum(1 for s in skills if s.get("suggestable"))
    marketplace_count = sum(1 for s in skills if s.get("source") == "marketplace")

    # Self-validate output shape before writing. The Rust skill-suggest matcher
    # at rust/src/cli.rs:951,965 requires both `idf` (root-level dict) and
    # per-skill `suggestable=true` to score anything. A regression that drops
    # either field silently returns "no match" for every prompt — and tick.sh's
    # mtime-only staleness check (24h) cannot detect that. Refuse to write a
    # degenerate index so the existing valid one stays in place until fixed.
    if marketplace_count > 0:
        problems: list[str] = []
        if suggestable_count == 0:
            problems.append("suggestable_count=0 (per-skill suggestable flag dropped?)")
        if not idf:
            problems.append("idf is empty (IDF computation missing?)")
        if problems:
            print(
                "build_ecc_index: REFUSING to write degenerate index "
                f"({marketplace_count} marketplace skills found but: "
                + "; ".join(problems) + "). "
                "Existing index left untouched. "
                "Inspect _is_suggestable / _compute_idf in this script.",
                file=sys.stderr,
            )
            return 1

    payload = {
        "built_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "skill_count": len(skills),
        "suggestable_count": suggestable_count,
        "idf": idf,
        "skills": skills,
    }

    out_path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    print(
        f"build_ecc_index: wrote {out_path} "
        f"(skill_count={len(skills)}, suggestable={suggestable_count}, "
        f"idf_terms={len(idf)})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
