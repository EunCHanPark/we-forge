#!/usr/bin/env python3
"""Create a git-backed checkpoint without moving the current branch.

Usage:
    python3 harness/scripts/git_checkpoint.py <checkpoint-name>

Example:
    python3 harness/scripts/git_checkpoint.py before-db-migration-004
    python3 harness/scripts/git_checkpoint.py before-deploy

Checkpoints are stored at refs/harness-checkpoints/<name>.
To restore: git checkout refs/harness-checkpoints/<name> -- .
To list:    git for-each-ref refs/harness-checkpoints/
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path


def _run_git(repo_root: Path, args: list[str], env: dict | None = None) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo_root), *args],
        check=True,
        capture_output=True,
        text=True,
        env=env,
    )
    return result.stdout.strip()


def _sanitize_name(name: str) -> str:
    sanitized = re.sub(r"[^a-zA-Z0-9._-]+", "-", name.strip()).strip("-")
    if not sanitized:
        raise ValueError("checkpoint name is empty after sanitization.")
    return sanitized


def _has_head(repo_root: Path) -> bool:
    result = subprocess.run(
        ["git", "-C", str(repo_root), "rev-parse", "--verify", "HEAD"],
        capture_output=True,
        text=True,
    )
    return result.returncode == 0


def _checkpoint_env(index_path: str) -> dict:
    env = os.environ.copy()
    env["GIT_INDEX_FILE"] = index_path
    env.setdefault("GIT_AUTHOR_NAME", "Harness Agent")
    env.setdefault("GIT_AUTHOR_EMAIL", "harness@local")
    env.setdefault("GIT_COMMITTER_NAME", env["GIT_AUTHOR_NAME"])
    env.setdefault("GIT_COMMITTER_EMAIL", env["GIT_AUTHOR_EMAIL"])
    return env


def create_checkpoint(
    repo_root: Path, name: str, message: str | None = None
) -> tuple[str, str]:
    checkpoint_name = _sanitize_name(name)
    checkpoint_ref = f"refs/harness-checkpoints/{checkpoint_name}"

    handle = tempfile.NamedTemporaryFile(prefix="harness-index-", delete=False)
    index_path = handle.name
    handle.close()
    os.unlink(index_path)

    env = _checkpoint_env(index_path)

    try:
        _run_git(repo_root, ["add", "-A", "--", "."], env=env)
        tree = _run_git(repo_root, ["write-tree"], env=env)

        commit_args = ["commit-tree", tree]
        if _has_head(repo_root):
            parent = _run_git(repo_root, ["rev-parse", "HEAD"])
            commit_args.extend(["-p", parent])
        commit_message = message or f"harness checkpoint: {checkpoint_name}"
        commit_args.extend(["-m", commit_message])
        commit = _run_git(repo_root, commit_args, env=env)
        _run_git(repo_root, ["update-ref", checkpoint_ref, commit])
    finally:
        try:
            os.unlink(index_path)
        except FileNotFoundError:
            pass

    return checkpoint_ref, commit


def main() -> None:
    parser = argparse.ArgumentParser(description="Create a harness git checkpoint.")
    parser.add_argument("name", help="Checkpoint name (e.g. before-db-migration-004)")
    parser.add_argument("--message", "-m", help="Optional commit message")
    parser.add_argument(
        "--repo", default=".", help="Path to git repo root (default: current dir)"
    )
    args = parser.parse_args()

    repo_root = Path(args.repo).resolve()
    if not (repo_root / ".git").exists():
        print(f"Error: {repo_root} is not a git repository.", file=sys.stderr)
        sys.exit(1)

    try:
        ref, commit = create_checkpoint(repo_root, args.name, args.message)
        print(f"Checkpoint created: {ref}")
        print(f"Commit: {commit}")
        print(f"Restore: git checkout {ref} -- .")
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
