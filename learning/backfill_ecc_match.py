#!/usr/bin/env python3
"""backfill_ecc_match.py — populate ecc_skill / ecc_source / match_score on
historical ledger.jsonl ECC_MATCH entries that pre-date 2026-04-26.

The agent spec was revised on 2026-04-26 to require these traceability
fields on every ECC_MATCH record. Records written before then are
mostly missing them. This one-shot backfill performs a best-effort
match of each historical ECC_MATCH slug against the ECC index, scoring
on token overlap. Backfilled entries get `backfilled: true` so they're
distinguishable from forward-recorded ones.

Usage:
    python3 backfill_ecc_match.py [--dry-run]

Reads:
    ~/.claude/learning/data/ledger.jsonl
    ~/.claude/learning/data/patterns.jsonl   (to recover pattern strings)
    ~/.we-forge/ecc-index.json                (target index for matching)

Writes (atomic):
    ~/.claude/learning/data/ledger.jsonl      (rewritten with new fields)
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile
from pathlib import Path

_TOKEN_RE = re.compile(r"[A-Za-z][A-Za-z0-9_-]{2,}")
_STOPWORDS = {
    "the", "and", "for", "with", "this", "that", "from", "into",
    "when", "where", "which", "after", "before",
    "are", "or", "of", "to", "in", "on", "at",
    "be", "by", "as", "it", "if", "you", "your", "use", "used",
}


def _tokens(text: str) -> set[str]:
    return {
        m.group(0).lower()
        for m in _TOKEN_RE.finditer(text or "")
        if len(m.group(0)) >= 4 and m.group(0).lower() not in _STOPWORDS
    }


def _read_jsonl(path: Path) -> list[dict]:
    if not path.exists():
        return []
    out: list[dict] = []
    with path.open(encoding="utf-8") as f:
        for ln in f:
            ln = ln.strip()
            if not ln:
                continue
            try:
                out.append(json.loads(ln))
            except json.JSONDecodeError:
                continue
    return out


def _atomic_write_jsonl(path: Path, rows: list[dict]) -> None:
    fd, tmp = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            for row in rows:
                f.write(json.dumps(row, ensure_ascii=False) + "\n")
        os.replace(tmp, path)
    except Exception:
        try:
            os.unlink(tmp)
        except FileNotFoundError:
            pass
        raise


def _score(slug: str, pattern: str, skill: dict) -> int:
    """Approximate the pattern-detector's scoring rules."""
    skill_tokens = set(skill.get("tokens", []))
    score = 0
    if skill.get("slug", "").lower() == slug.lower():
        score += 5
    slug_toks = {t for t in slug.split("-") if len(t) >= 3}
    if slug_toks and skill_tokens:
        overlap = len(slug_toks & skill_tokens)
        if overlap >= max(2, len(slug_toks) // 2):
            score += 2
    pat_tokens = _tokens(pattern)
    if pat_tokens and skill_tokens:
        if len(pat_tokens & skill_tokens) >= 2:
            score += 2
    head = pattern.split(":", 1)[0].split()[0].lower() if pattern else ""
    if head and head in skill.get("name", "").lower():
        score += 3
    return score


def _best_match(slug: str, pattern: str, index: list[dict]) -> tuple:
    best = None
    best_score = 0
    for s in index:
        sc = _score(slug, pattern, s)
        if sc > best_score:
            best_score = sc
            best = s
    return best, best_score


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    home = Path.home()
    ledger_path = home / ".claude" / "learning" / "data" / "ledger.jsonl"
    patterns_path = home / ".claude" / "learning" / "data" / "patterns.jsonl"
    index_path = Path(os.environ.get(
        "WE_FORGE_HOME", str(home / ".we-forge"))) / "ecc-index.json"

    if not ledger_path.exists():
        print(f"ledger not found: {ledger_path}")
        return 1
    if not index_path.exists():
        print(f"ecc-index not found: {index_path} — run build_ecc_index.py first")
        return 1

    ledger = _read_jsonl(ledger_path)
    patterns = {p.get("pattern", ""): p for p in _read_jsonl(patterns_path)}
    skills = json.loads(index_path.read_text(encoding="utf-8")).get("skills", [])

    def _slug(p: str) -> str:
        s = re.sub(r"[^A-Za-z0-9]+", "-", p.lower()).strip("-")
        return (s[:60] or "pattern").strip("-") or "pattern"

    slug_to_pattern: dict = {_slug(p): p for p in patterns}

    backfilled = 0
    fully_traced = 0
    for row in ledger:
        if row.get("decision") != "ECC_MATCH":
            continue
        # Already-tracked records may pre-date the ecc_source / match_score
        # additions. Skip only when ALL three fields are populated.
        if (row.get("ecc_skill")
                and row.get("ecc_source")
                and row.get("match_score") is not None):
            fully_traced += 1
            continue
        slug = row.get("slug", "")
        pattern = slug_to_pattern.get(slug, slug)
        # If ecc_skill already known, find that exact skill in index for
        # ecc_source; otherwise score-match from scratch.
        existing_name = row.get("ecc_skill", "")
        skill = None
        score = 0
        if existing_name:
            for s in skills:
                if s.get("name") == existing_name or s.get("slug") == existing_name.lower():
                    skill = s
                    score = _score(slug, pattern, s)
                    break
        if skill is None:
            skill, score = _best_match(slug, pattern, skills)
        if skill:
            if not row.get("ecc_skill"):
                row["ecc_skill"] = skill.get("name", "")
            if not row.get("ecc_source"):
                row["ecc_source"] = skill.get("source", "")
            if row.get("match_score") is None:
                row["match_score"] = score
            row["backfilled"] = True
            backfilled += 1

    total = sum(1 for r in ledger if r.get("decision") == "ECC_MATCH")
    print(f"backfill_ecc_match: total_ECC_MATCH={total} "
          f"already_fully_traced={fully_traced} backfilled={backfilled}")

    if args.dry_run:
        print("dry-run: not writing")
        return 0

    _atomic_write_jsonl(ledger_path, ledger)
    print(f"wrote {ledger_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
