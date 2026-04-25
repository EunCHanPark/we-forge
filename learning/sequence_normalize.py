#!/usr/bin/env python3
"""sequence_normalize.py — extract N-gram sequence candidates from events.jsonl.

This is the multi-step workflow learning component. While normalize.py
treats each event as an independent pattern, sequence_normalize.py groups
events by session within a 5-minute sliding window and extracts N-gram
sequences (N=2..4) of canonical tool invocations.

**Shadow mode** (default): writes candidates to sequence_candidates.jsonl
and emits one ledger line per surfaced candidate with `decision="SEQ_CANDIDATE"`.
It does NOT enqueue anything for synthesis — the auditor is not invoked.
This lets us observe the candidate distribution for ~1 week before deciding
whether to promote sequences to the standard PASS/REJECT pipeline.

**Hard gates** (advisor-prescribed) prevent false-positive explosion:
  MIN_SUPPORT             3   distinct sessions must contain the same sequence
  MAX_N                   4   no sequences longer than 4 tools
  WINDOW_SECONDS        300   only events within 5 min of each other group
  SELF_LOOP_COLLAPSE     yes  Edit->Edit->Edit collapses to single Edit node

Output schema (sequence_candidates.jsonl, atomic-rewritten each tick):

    {
      "sequence":         ["Edit:<PATH>.py", "Bash:pytest", "Bash:git commit -m <STR>"],
      "head_tool":        "Edit",
      "n":                3,
      "support":          5,
      "distinct_sessions":4,
      "first_seen":       "2026-04-25T08:00:00Z",
      "last_seen":        "2026-04-26T11:30:00Z",
      "slug":             "seq-edit-bash-pytest-bash-git-commit"
    }

Ledger emit (one line per *new* candidate this tick):

    {"ts":"<iso>","decision":"SEQ_CANDIDATE","slug":"seq-...","n":3,
     "support":5,"shadow_mode":true}
"""
from __future__ import annotations

import json
import os
import re
import sys
import tempfile
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

# Reuse normalize.canonicalize so sequences use the same canonical form
# even though events.jsonl is append-only and stores raw with normalized=None.
sys.path.insert(0, str(Path(__file__).resolve().parent))
try:
    from normalize import canonicalize as _canon  # type: ignore
except Exception:
    def _canon(source: str, raw: str) -> str:  # fallback no-op
        return raw or ""

# Hard gates (advisor-prescribed).
MIN_SUPPORT = 3
MIN_N = 2
MAX_N = 4
WINDOW_SECONDS = 300


def _now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _parse_iso(ts: str) -> float:
    if not ts:
        return 0.0
    try:
        return datetime.fromisoformat(ts.replace("Z", "+00:00")).timestamp()
    except (ValueError, TypeError):
        return 0.0


def _slug(seq: list[str]) -> str:
    joined = "-".join(seq)
    s = re.sub(r"[^A-Za-z0-9]+", "-", joined.lower()).strip("-")
    return ("seq-" + (s[:80] or "empty")).strip("-")


def _atomic_write(path: Path, lines):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            for ln in lines:
                if not ln.endswith("\n"):
                    ln += "\n"
                f.write(ln)
        os.replace(tmp, path)
    except Exception:
        try:
            os.unlink(tmp)
        except FileNotFoundError:
            pass
        raise


def _read_jsonl(path: Path) -> list[dict]:
    if not path.exists():
        return []
    out = []
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


def _collapse_self_loops(seq: list[str]) -> list[str]:
    """Edit->Edit->Edit becomes [Edit] (single occurrence)."""
    out: list[str] = []
    prev = None
    for tok in seq:
        if tok != prev:
            out.append(tok)
        prev = tok
    return out


def _group_by_session(events: list[dict]) -> dict:
    by_sid: dict = defaultdict(list)
    for ev in events:
        sid = ev.get("session_id", "")
        ts = _parse_iso(ev.get("ts", ""))
        if not sid or not ts:
            continue
        # events.jsonl is append-only: `normalized` is computed in-memory by
        # normalize.py but not persisted, so most rows have normalized=None.
        # Canonicalize on the fly using the same function for consistency.
        norm = ev.get("normalized")
        if not norm:
            raw = ev.get("raw") or ""
            if not raw:
                continue
            norm = _canon(ev.get("source", "bash"), raw)
        if not norm:
            continue
        by_sid[sid].append((ts, norm))
    for sid, arr in by_sid.items():
        arr.sort(key=lambda r: r[0])
    return by_sid


def _extract_ngrams_from_session(events) -> list[list[str]]:
    """Slide a WINDOW_SECONDS window over a session and emit N-grams."""
    seqs: list[list[str]] = []
    if not events:
        return seqs
    n = len(events)
    for i in range(n):
        end_ts = events[i][0] + WINDOW_SECONDS
        window: list[str] = [events[i][1]]
        j = i + 1
        while j < n and events[j][0] <= end_ts:
            window.append(events[j][1])
            j += 1
        collapsed = _collapse_self_loops(window)
        for length in range(MIN_N, MAX_N + 1):
            if len(collapsed) < length:
                break
            for k in range(0, len(collapsed) - length + 1):
                seqs.append(collapsed[k:k + length])
    return seqs


def main(argv: list[str]) -> int:
    data_dir = Path(os.environ.get(
        "CLAUDE_LEARNING_DATA", str(Path.home() / ".claude/learning/data")))
    events_path = data_dir / "events.jsonl"
    candidates_path = data_dir / "sequence_candidates.jsonl"
    ledger_path = data_dir / "ledger.jsonl"

    events = _read_jsonl(events_path)
    if not events:
        _atomic_write(candidates_path, [])
        print("sequence_normalize: events=0 candidates=0")
        return 0

    by_sid = _group_by_session(events)

    seq_agg: dict = {}
    for sid, evs in by_sid.items():
        ngrams = _extract_ngrams_from_session(evs)
        seen_in_session = {tuple(s) for s in ngrams}
        for tup in seen_in_session:
            bucket = seq_agg.setdefault(tup, {
                "sessions": set(),
                "first_seen_ts": float("inf"),
                "last_seen_ts": 0.0,
            })
            bucket["sessions"].add(sid)
            ts0 = evs[0][0]
            ts1 = evs[-1][0]
            if ts0 < bucket["first_seen_ts"]:
                bucket["first_seen_ts"] = ts0
            if ts1 > bucket["last_seen_ts"]:
                bucket["last_seen_ts"] = ts1

    candidates: list[dict] = []
    for tup, b in seq_agg.items():
        distinct = len(b["sessions"])
        if distinct < MIN_SUPPORT:
            continue
        seq = list(tup)
        head = seq[0].split(":", 1)[0] if ":" in seq[0] else seq[0].split()[0]
        candidates.append({
            "sequence":          seq,
            "head_tool":         head,
            "n":                 len(seq),
            "support":           distinct,
            "distinct_sessions": distinct,
            "first_seen":        datetime.fromtimestamp(
                b["first_seen_ts"], tz=timezone.utc
            ).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "last_seen":         datetime.fromtimestamp(
                b["last_seen_ts"], tz=timezone.utc
            ).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "slug":              _slug(seq),
        })

    candidates.sort(key=lambda c: (-c["support"], c["n"], c["slug"]))

    existing = {c.get("slug"): c for c in _read_jsonl(candidates_path)}
    new_slugs = [c["slug"] for c in candidates if c["slug"] not in existing]

    _atomic_write(candidates_path,
                  [json.dumps(c, ensure_ascii=False) for c in candidates])

    if new_slugs and ledger_path.parent.exists():
        with ledger_path.open("a", encoding="utf-8") as f:
            now = _now_iso()
            for c in candidates:
                if c["slug"] not in new_slugs:
                    continue
                f.write(json.dumps({
                    "ts":          now,
                    "decision":    "SEQ_CANDIDATE",
                    "slug":        c["slug"],
                    "n":           c["n"],
                    "support":     c["support"],
                    "shadow_mode": True,
                }, ensure_ascii=False) + "\n")

    print(f"sequence_normalize: events={len(events)} sessions={len(by_sid)} "
          f"candidates={len(candidates)} new_this_tick={len(new_slugs)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
