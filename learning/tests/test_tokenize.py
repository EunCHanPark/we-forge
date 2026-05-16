"""Tokenizer regression suite — Python side.

Locks in the current observable behavior of `build_ecc_index._tokenize`
on the same anchor inputs that appear in
`rust/src/cli/skill_suggest.rs` `mod tests`.

The two suites do NOT assert that Rust and Python produce identical token
sets — they intentionally differ:

    Python `_tokenize`: emits compound + each part on `-`/`_` boundaries,
                       preserves raw token (suffix-strip only for reverse
                       Korean synonym lookup), uses re.finditer ordering.
    Rust `tokenize`:    emits one concatenated token, then applies suffix
                       stripping (ing/ies/es/s) before pushing.

Both sides are pinned so unintentional drift on either side gets caught.

When you legitimately change `_tokenize`:
    1. python3 -m unittest learning.tests.test_tokenize   (update here)
    2. cargo test -p we-forgectl tokenizer                (update Rust side)
    3. python3 learning/build_ecc_index.py                (rebuild index)
    4. we-forgectl skill-regressions                      (confirm anchors)
"""
import os
import sys
import unittest
from pathlib import Path

# Allow `python3 -m unittest learning.tests.test_tokenize` to import the
# tokenizer when the repo root is cwd. Also supports direct execution from
# the learning/ directory.
_REPO_ROOT = Path(__file__).resolve().parents[2]
if str(_REPO_ROOT / "learning") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "learning"))

from build_ecc_index import _tokenize  # noqa: E402


class TokenizerAnchorTests(unittest.TestCase):
    def test_empty_string(self):
        self.assertEqual(_tokenize(""), [])

    def test_length_floor_three(self):
        # Tokens of length <3 are dropped by `_TOKEN_RE`'s {2,} quantifier
        # (which requires 2+ chars AFTER the initial letter, so 3+ total).
        self.assertEqual(_tokenize("a bc abc abcd abcde"), ["abc", "abcd", "abcde"])

    def test_suffix_stripping_only_for_reverse_synonym(self):
        # Unlike Rust, Python emits RAW tokens (no suffix-strip for the
        # primary output stream). Suffix variants are only used to look up
        # reverse synonyms.
        self.assertEqual(
            _tokenize("deploying deploys runs running stripped"),
            # 'deploying' → reverse-synonym lookup strips ing → 'deploy' →
            # finds Korean '배포'. So 배포 is appended at the end.
            ["deploying", "deploys", "runs", "running", "stripped", "배포"],
        )

    def test_compound_splitting_on_hyphen(self):
        # Compound tokens are emitted BOTH whole AND split.
        self.assertEqual(
            _tokenize("kotlin-coroutines-flows"),
            ["kotlin-coroutines-flows", "kotlin", "coroutines", "flows"],
        )

    def test_stopwords_dropped(self):
        self.assertEqual(_tokenize("the git and the and for the"), ["git"])

    def test_korean_solo_with_synonym_expand(self):
        # '배포' → forward expand: ['deploy', 'deployment', 'release'].
        # Then the reverse-synonym pass finds 'release' is also a value for
        # the separate key '릴리즈', so '릴리즈' is appended. This two-way
        # bridge is intentional — it gives Korean prompts more synonym surface.
        self.assertEqual(
            _tokenize("배포"),
            ["배포", "deploy", "deployment", "release", "릴리즈"],
        )

    def test_korean_single_syllable_dropped(self):
        # _HANGUL_RE requires 2+ syllables; single-syllable runs match nothing.
        self.assertEqual(_tokenize("팟 앱 가"), [])

    def test_mixed_korean_english_anchor(self):
        # Mirrors Rust `tokenizer_mixed_korean_english_anchor`.
        # English tokens first (text order), then Korean tokens with
        # synonyms. Note: Python's reverse-synonym pass does NOT add
        # English back to a token that already arrived via Korean expansion
        # (the dedupe guard prevents it).
        self.assertEqual(
            _tokenize("PostgreSQL 쿼리 최적화 인덱스"),
            [
                "postgresql",
                "쿼리", "query",
                "최적화", "optimization", "optimize",
                "인덱스", "index",
                "색인",  # reverse-synonym: index→색인 (Python only)
            ],
        )

    def test_dedupe_within_one_pass(self):
        # 'query' from raw match, 'Query'/'QUERY' dedupe via .lower(),
        # 'querys' emitted as-is (no suffix-strip on primary output).
        # Then the reverse-synonym pass maps English 'query' → Korean '쿼리'
        # (since '쿼리' is the canonical Korean for 'query' in the synonym map),
        # so '쿼리' is appended as the cross-language bridge.
        self.assertEqual(
            _tokenize("query Query QUERY querys"),
            ["query", "querys", "쿼리"],
        )

    def test_regression_anchor_canary_watch(self):
        # Same prompt as skill-suggest-regressions.json id=en-deploy-verify.
        # Anchor: tokens emitted must include the deploy/verify/endpoint
        # cluster that scores canary-watch as top-1.
        toks = set(_tokenize("verify deployed URL endpoint SSE static asset"))
        for required in ["verify", "deployed", "endpoint", "static", "asset"]:
            self.assertIn(required, toks, f"missing token {required!r}")

    def test_regression_anchor_ko_canary_watch(self):
        # Anchor: pure-Korean canary prompt must yield enough English
        # synonyms (deploy, verify, regression, test, monitor) to bridge
        # to the English-only canary-watch description.
        toks = set(_tokenize("배포 검증 회귀 테스트 모니터링"))
        for required in ["deploy", "verify", "regression", "test", "monitor"]:
            self.assertIn(required, toks, f"missing English bridge {required!r}")


if __name__ == "__main__":
    unittest.main()
