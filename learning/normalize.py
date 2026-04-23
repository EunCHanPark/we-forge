#!/usr/bin/env python3
"""normalize.py — canonicalize events and produce the pattern frequency table.

Data files this script touches (under ~/.claude/learning/data/ by default):

  events.jsonl
    one JSON object per line, schema:
      {
        "ts":          "2026-04-23T12:00:00Z",     # ISO-8601 UTC
        "session_id":  "abc-123",
        "source":      "bash" | "transcript" | "stophook",
        "raw":         str,
        "normalized":  str | null
      }

  patterns.jsonl
    one JSON object per normalized pattern:
      {
        "pattern":            "git status",
        "count":              3,
        "first_seen":         "2026-04-23T12:00:00Z",
        "last_seen":          "2026-04-23T13:00:00Z",
        "sample_session_ids": ["abc", "def", "ghi"],
        "sample_raw":         ["git status", ...]   # <= 5, already redacted
      }

  promotion_queue.jsonl
    one JSON object per candidate pattern awaiting synthesis:
      {
        "pattern":            "git status",
        "samples":            ["git status"],
        "sample_session_ids": ["abc", "def", "ghi"],
        "first_seen":         "2026-04-23T12:00:00Z",
        "last_seen":          "2026-04-23T13:00:00Z",
        "count":              3,
        "revise_count":       0,
        "enqueued_at":        "2026-04-23T13:00:00Z",
        "slug":               "git-status"
      }

  rejected.txt
    one normalized pattern per line; never re-queue.

Exclusion rules (in order):
  1. events whose raw fails is_secret() are dropped (not masked).
  2. patterns whose slug already exists in ~/.claude/skills/learned/ are skipped.
  3. patterns already in promotion_queue.jsonl are skipped.
  4. patterns listed in rejected.txt are skipped.
  5. patterns with fewer than 3 distinct session_ids are held back.
"""
from __future__ import annotations

import json
import math
import os
import re
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable


# ---------------------------------------------------------------------------
# Redaction (kept in sync with learning/redact.sh).
# ---------------------------------------------------------------------------
_SECRET_KV = re.compile(
    r"(api[_-]?key|passwd|password|secret|token|bearer|authorization)\s*[:=]\s*\S+",
    re.IGNORECASE,
)
_SECRET_ENV = re.compile(
    r"(ANTHROPIC_API_KEY|OPENAI_API_KEY|AWS_SECRET_ACCESS_KEY|AWS_ACCESS_KEY_ID|"
    r"GITHUB_TOKEN|GH_TOKEN|HF_TOKEN|SLACK_TOKEN|NPM_TOKEN|STRIPE_KEY|STRIPE_SECRET)"
)
_SECRET_PREFIX = re.compile(
    r"(^|[^A-Za-z0-9])("
    r"sk-[A-Za-z0-9_-]{16,}|"
    r"ghp_[A-Za-z0-9]{20,}|ghs_[A-Za-z0-9]{20,}|gho_[A-Za-z0-9]{20,}|"
    r"xox[bpsa]-[A-Za-z0-9-]{10,}|"
    r"AKIA[0-9A-Z]{16}"
    r")"
)
_LONG_TOKEN = re.compile(r"[A-Za-z0-9+/=_-]{32,}")


def _shannon(s: str) -> float:
    if not s:
        return 0.0
    freq: dict[str, int] = {}
    for c in s:
        freq[c] = freq.get(c, 0) + 1
    n = len(s)
    return -sum((f / n) * math.log2(f / n) for f in freq.values())


def is_secret(line: str) -> bool:
    if _SECRET_KV.search(line):
        return True
    if _SECRET_ENV.search(line):
        return True
    if _SECRET_PREFIX.search(line):
        return True
    tokens = _LONG_TOKEN.findall(line)
    if tokens:
        longest = max(tokens, key=len)
        if _shannon(longest) >= 4.0:
            return True
    return False


# ---------------------------------------------------------------------------
# Canonicalization.
# ---------------------------------------------------------------------------
_ABS_PATH = re.compile(r"(?<![A-Za-z0-9])(?:~|/)[A-Za-z0-9_./\-]+")
_UUID = re.compile(
    r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b"
)
_HEX = re.compile(r"\b[0-9a-f]{7,40}\b")
_NUM = re.compile(r"\b\d+\b")
_SINGLE_Q = re.compile(r"'[^']*'")
_DOUBLE_Q = re.compile(r'"[^"]*"')


def _canonicalize_shell(cmd: str) -> str:
    s = cmd.strip()
    s = _UUID.sub("<UUID>", s)
    s = _SINGLE_Q.sub("<STR>", s)
    s = _DOUBLE_Q.sub("<STR>", s)
    s = _ABS_PATH.sub("<PATH>", s)
    s = _HEX.sub("<HEX>", s)
    s = _NUM.sub("<N>", s)
    s = re.sub(r"\s+", " ", s)
    return s.strip()


def _canonicalize_transcript(raw: str) -> str:
    try:
        obj = json.loads(raw)
    except (json.JSONDecodeError, TypeError):
        return _canonicalize_shell(raw)

    tool = obj.get("tool") or obj.get("tool_name") or "tool"
    inp = obj.get("input") or obj.get("tool_input") or ""

    if tool == "Bash":
        cmd = inp.get("command", "") if isinstance(inp, dict) else str(inp)
        return f"Bash:{_canonicalize_shell(cmd)}"
    if tool in ("Edit", "Write", "Read"):
        path = inp.get("file_path", "") if isinstance(inp, dict) else ""
        ext = os.path.splitext(path)[1] or "<none>"
        return f"{tool}:<PATH>{ext}"
    if tool == "Grep":
        return "Grep:<STR>"
    if tool == "Glob":
        return "Glob:<STR>"
    return f"{tool}:<opaque>"


def canonicalize(source: str, raw: str) -> str:
    if source == "transcript":
        return _canonicalize_transcript(raw)
    return _canonicalize_shell(raw)


# ---------------------------------------------------------------------------
# File IO helpers.
# ---------------------------------------------------------------------------
def _atomic_write(path: Path, lines: Iterable[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_path = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            for line in lines:
                if not line.endswith("\n"):
                    line += "\n"
                f.write(line)
        os.replace(tmp_path, path)
    except Exception:
        try:
            os.unlink(tmp_path)
        except FileNotFoundError:
            pass
        raise


def _read_jsonl(path: Path) -> list[dict]:
    if not path.exists():
        return []
    out: list[dict] = []
    with path.open("r", encoding="utf-8") as f:
        for ln in f:
            ln = ln.strip()
            if not ln:
                continue
            try:
                out.append(json.loads(ln))
            except json.JSONDecodeError:
                continue
    return out


def _load_learned_skill_slugs(learned_dir: Path) -> set[str]:
    slugs: set[str] = set()
    if not learned_dir.exists():
        return slugs
    for child in learned_dir.iterdir():
        if child.is_dir() and (child / "SKILL.md").exists():
            slugs.add(child.name)
    return slugs


def _load_rejected(path: Path) -> set[str]:
    if not path.exists():
        return set()
    with path.open("r", encoding="utf-8") as f:
        return {ln.strip() for ln in f if ln.strip()}


def _pattern_to_slug(pattern: str) -> str:
    slug = re.sub(r"[^A-Za-z0-9]+", "-", pattern.lower()).strip("-")
    return (slug[:60] or "pattern").strip("-") or "pattern"


def _now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


# ---------------------------------------------------------------------------
# Main.
# ---------------------------------------------------------------------------
def main(argv: list[str]) -> int:
    data_dir = Path(os.environ.get(
        "CLAUDE_LEARNING_DATA", str(Path.home() / ".claude/learning/data")))
    learned_dir = Path(os.environ.get(
        "CLAUDE_LEARNED_SKILLS", str(Path.home() / ".claude/skills/learned")))

    events_path = data_dir / "events.jsonl"
    patterns_path = data_dir / "patterns.jsonl"
    queue_path = data_dir / "promotion_queue.jsonl"
    rejected_path = data_dir / "rejected.txt"

    data_dir.mkdir(parents=True, exist_ok=True)

    events = _read_jsonl(events_path)
    if not events:
        _atomic_write(patterns_path, [])
        print("normalize: events=0 patterns=0")
        return 0

    rewritten: list[str] = []
    valid_events: list[dict] = []
    dropped_secret = 0
    for ev in events:
        raw = ev.get("raw") or ""
        if is_secret(raw):
            dropped_secret += 1
            continue
        if not ev.get("normalized"):
            ev["normalized"] = canonicalize(ev.get("source", "bash"), raw)
        rewritten.append(json.dumps(ev, ensure_ascii=False))
        valid_events.append(ev)

    _atomic_write(events_path, rewritten)

    # Aggregate.
    agg: dict[str, dict] = {}
    for ev in valid_events:
        pat = ev.get("normalized") or ""
        if not pat:
            continue
        bucket = agg.setdefault(pat, {
            "pattern": pat,
            "count": 0,
            "first_seen": ev.get("ts", _now_iso()),
            "last_seen": ev.get("ts", _now_iso()),
            "sample_session_ids": [],
            "sample_raw": [],
        })
        bucket["count"] += 1
        ts = ev.get("ts", "")
        if ts and ts < bucket["first_seen"]:
            bucket["first_seen"] = ts
        if ts and ts > bucket["last_seen"]:
            bucket["last_seen"] = ts
        sid = ev.get("session_id", "")
        if sid and sid not in bucket["sample_session_ids"] \
                and len(bucket["sample_session_ids"]) < 10:
            bucket["sample_session_ids"].append(sid)
        raw = ev.get("raw", "")
        if raw and raw not in bucket["sample_raw"] and len(bucket["sample_raw"]) < 5:
            bucket["sample_raw"].append(raw)

    patterns = sorted(agg.values(), key=lambda p: (-p["count"], p["pattern"]))
    _atomic_write(patterns_path, [json.dumps(p, ensure_ascii=False) for p in patterns])

    # Build promotion queue additions.
    learned = _load_learned_skill_slugs(learned_dir)
    rejected = _load_rejected(rejected_path)
    queue = _read_jsonl(queue_path)
    queued_patterns = {q["pattern"] for q in queue if "pattern" in q}

    added = 0
    for p in patterns:
        if p["count"] < 3:
            continue
        if len(p["sample_session_ids"]) < 3:
            continue
        slug = _pattern_to_slug(p["pattern"])
        if slug in learned:
            continue
        if p["pattern"] in rejected:
            continue
        if p["pattern"] in queued_patterns:
            continue
        queue.append({
            "pattern": p["pattern"],
            "samples": p["sample_raw"],
            "sample_session_ids": p["sample_session_ids"],
            "first_seen": p["first_seen"],
            "last_seen": p["last_seen"],
            "count": p["count"],
            "revise_count": 0,
            "enqueued_at": _now_iso(),
            "slug": slug,
        })
        queued_patterns.add(p["pattern"])
        added += 1

    _atomic_write(queue_path, [json.dumps(q, ensure_ascii=False) for q in queue])

    print(f"normalize: events={len(valid_events)} dropped_secret={dropped_secret} "
          f"patterns={len(patterns)} queue_len={len(queue)} added={added}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
