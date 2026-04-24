#!/usr/bin/env python3
"""Validate harness tree for drift and unfilled placeholders.

Checks:
  (a) Path drift: the pattern ``harness/apps/`` must not appear anywhere
      in the harness tree. The correct layout is ``apps/<app>/harness/``.
  (b) Placeholder leak: double-brace placeholder literals (as written by
      init_harness.py before substitution) must not appear in instantiated
      harness files (``harness/core/``, ``apps/*/harness/``). Those files
      are the materialised copies, not templates.

Usage:
    python harness/scripts/validate_harness.py          # exit 0 = PASS, 1 = FAIL
    python harness/scripts/validate_harness.py --repo-root /path/to/repo
    python harness/scripts/validate_harness.py --verbose

Exit codes:
    0  All checks passed.
    1  One or more violations detected.
    2  Invocation error (missing paths, bad args).
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

DRIFT_PATTERN = re.compile(r"harness/apps/")
PLACEHOLDER_PATTERN = re.compile(r"\{\{[^{}\n]+\}\}")

TEXT_SUFFIXES = {
    ".md",
    ".py",
    ".txt",
    ".yaml",
    ".yml",
    ".json",
    ".toml",
    ".ini",
    ".cfg",
    ".sh",
    ".ps1",
}

# Files inside harness/ that are allowed to contain double-brace placeholder
# literals as self-documentation of placeholder syntax. Add sparingly.
PLACEHOLDER_ALLOWLIST: set[str] = {
    "harness/scripts/validate_harness.py",
}

# Files allowed to contain the literal drift pattern as documentation or regex
# source (this validator itself). Add sparingly.
DRIFT_ALLOWLIST: set[str] = {
    "harness/scripts/validate_harness.py",
}


@dataclass
class Violation:
    check: str
    path: Path
    line_no: int
    snippet: str

    def format(self, repo_root: Path) -> str:
        rel = self.path.relative_to(repo_root).as_posix()
        return f"[{self.check}] {rel}:{self.line_no}: {self.snippet}"


def _iter_text_files(roots: list[Path]) -> list[Path]:
    files: list[Path] = []
    for root in roots:
        if not root.exists():
            continue
        for path in root.rglob("*"):
            if not path.is_file():
                continue
            if path.suffix.lower() not in TEXT_SUFFIXES:
                continue
            # Exclude plans/: these are working documents that may legitimately
            # reference drift patterns or placeholder syntax as documentation.
            # The validator targets prescriptive harness content only
            # (docs/, workflows/, roles/, templates/, platforms/, scripts/,
            # references/).
            if "plans" in path.parts:
                continue
            files.append(path)
    return files


def _read_lines(path: Path) -> list[str]:
    try:
        return path.read_text(encoding="utf-8").splitlines()
    except UnicodeDecodeError:
        return []


def check_drift(files: list[Path], repo_root: Path) -> list[Violation]:
    violations: list[Violation] = []
    for path in files:
        rel = path.relative_to(repo_root).as_posix()
        if rel in DRIFT_ALLOWLIST:
            continue
        for idx, line in enumerate(_read_lines(path), start=1):
            if DRIFT_PATTERN.search(line):
                violations.append(
                    Violation(
                        check="drift",
                        path=path,
                        line_no=idx,
                        snippet=line.strip()[:200],
                    )
                )
    return violations


def check_placeholders(files: list[Path], repo_root: Path) -> list[Violation]:
    violations: list[Violation] = []
    for path in files:
        rel = path.relative_to(repo_root).as_posix()
        if rel in PLACEHOLDER_ALLOWLIST:
            continue
        for idx, line in enumerate(_read_lines(path), start=1):
            if PLACEHOLDER_PATTERN.search(line):
                violations.append(
                    Violation(
                        check="placeholder",
                        path=path,
                        line_no=idx,
                        snippet=line.strip()[:200],
                    )
                )
    return violations


def _resolve_harness_roots(repo_root: Path) -> list[Path]:
    roots = [repo_root / "harness"]
    apps_dir = repo_root / "apps"
    if apps_dir.exists():
        for app in sorted(apps_dir.iterdir()):
            candidate = app / "harness"
            if candidate.is_dir():
                roots.append(candidate)
    return roots


def main(argv: list[str] | None = None) -> int:
    if hasattr(sys.stdout, "reconfigure"):
        try:
            sys.stdout.reconfigure(encoding="utf-8", errors="replace")
            sys.stderr.reconfigure(encoding="utf-8", errors="replace")
        except (OSError, AttributeError):
            pass

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="Repository root (default: inferred from script location).",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="List scanned files before running checks.",
    )
    args = parser.parse_args(argv)

    repo_root: Path = args.repo_root.resolve()
    if not repo_root.is_dir():
        print(f"ERROR: repo root does not exist: {repo_root}", file=sys.stderr)
        return 2

    roots = _resolve_harness_roots(repo_root)
    files = _iter_text_files(roots)

    if args.verbose:
        print(f"Repo root: {repo_root}")
        print("Scanned roots:")
        for root in roots:
            print(f"  - {root.relative_to(repo_root).as_posix()}")
        print(f"Text files scanned: {len(files)}")

    drift = check_drift(files, repo_root)
    placeholders = check_placeholders(files, repo_root)

    if drift:
        print(f"FAIL: drift violations ({len(drift)}):")
        for v in drift:
            print(f"  {v.format(repo_root)}")
    if placeholders:
        print(f"FAIL: placeholder leaks ({len(placeholders)}):")
        for v in placeholders:
            print(f"  {v.format(repo_root)}")

    if drift or placeholders:
        return 1

    print(f"OK: scanned {len(files)} files; no drift or placeholder leaks.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
