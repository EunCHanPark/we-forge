//! Prompt + skill-description tokenizer.
//!
//! Two parallel token streams: ASCII alpha runs (suffix-stripped) and
//! Hangul syllable runs (≥2 syllables, synonym-expanded). The tokenizer
//! is the single point where prompt text becomes the token set used by
//! both `rank` (query side) and `learning/build_ecc_index.py` (index side).
//! Cross-language parity is asserted by `learning/tests/test_tokenize.py`.

use std::collections::HashSet;

const STOPWORDS: &[&str] = &[
    "the","and","for","with","this","that","from","into","when","where","which",
    "after","before","are","or","of","to","in","on","at","be","by","as","it",
    "if","you","your","use","used",
];

// Korean → English synonym map mirrored from learning/build_ecc_index.py.
// When a Korean token appears in the prompt, the English equivalents are
// also emitted so a Korean-only prompt can match a skill whose description
// is English-only. Keep entries small and high-precision.
pub const KO_EN_SYNONYMS: &[(&str, &[&str])] = &[
    ("검증", &["verify", "verification"]),
    ("검사", &["check", "inspection"]),
    ("확인", &["verify", "check"]),
    ("배포", &["deploy", "deployment", "release"]),
    ("릴리즈", &["release"]),
    ("회귀", &["regression"]),
    ("엔드포인트", &["endpoint"]),
    ("정적", &["static"]),
    ("자산", &["asset"]),
    ("모니터", &["monitor", "monitoring"]),
    ("모니터링", &["monitor", "monitoring"]),
    ("테스트", &["test", "testing"]),
    ("보안", &["security", "secure"]),
    ("성능", &["performance"]),
    ("리뷰", &["review"]),
    ("코드리뷰", &["review", "code"]),
    ("쿼리", &["query"]),
    ("스키마", &["schema"]),
    ("마이그레이션", &["migration"]),
    ("개발", &["development", "dev"]),
    ("프롬프트", &["prompt"]),
    ("에이전트", &["agent"]),
    ("스킬", &["skill"]),
    ("워크플로", &["workflow"]),
    ("워크플로우", &["workflow"]),
    ("디버그", &["debug", "debugging"]),
    ("디버깅", &["debug", "debugging"]),
    ("패턴", &["pattern"]),
    ("색인", &["index", "indexing"]),
    ("인덱스", &["index"]),
    ("로그", &["log", "logging"]),
    ("리팩토링", &["refactor", "refactoring"]),
    ("최적화", &["optimization", "optimize"]),
    ("캐시", &["cache", "caching"]),
    ("데이터베이스", &["database"]),
    ("데이터", &["data"]),
    ("프론트엔드", &["frontend"]),
    ("백엔드", &["backend"]),
    ("도커", &["docker"]),
    ("컨테이너", &["container"]),
    ("빌드", &["build"]),
    ("린트", &["lint", "linting"]),
    ("린터", &["linter"]),
    ("타입", &["type"]),
    ("함수", &["function"]),
    ("컴포넌트", &["component"]),
    ("라이브러리", &["library"]),
    ("프레임워크", &["framework"]),
    ("디자인", &["design"]),
    ("시스템", &["system"]),
    ("스타일", &["style", "styling"]),
    ("일관성", &["consistency"]),
    ("비주얼", &["visual"]),
    ("문서", &["documentation", "docs"]),
    ("발표", &["presentation", "slide"]),
    ("슬라이드", &["slide", "presentation"]),
    ("그래프", &["graph"]),
    ("노트", &["note"]),
    ("지식", &["knowledge"]),
    ("기억", &["memory"]),
    ("메모리", &["memory"]),
    ("세션", &["session"]),
    ("이메일", &["email", "mail"]),
    ("메일", &["mail", "email"]),
    ("결제", &["billing", "payment"]),
    ("청구", &["billing"]),
    ("환불", &["refund"]),
    ("고객", &["customer"]),
    ("재고", &["inventory"]),
    ("물류", &["logistics", "shipping"]),
    ("반품", &["return"]),
];

/// True if char is in the Hangul syllable block (가-힣).
fn is_hangul(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c)
}

pub(super) fn tokenize(text: &str) -> Vec<String> {
    // Two parallel token streams:
    //   1. ASCII alpha runs (existing behavior; suffix-strips plurals/-ing)
    //   2. Hangul syllable runs (≥2 syllables) — new
    // Korean tokens are also expanded via KO_EN_SYNONYMS so a Korean-only
    // prompt can hit English-only skill descriptions.
    let stops: HashSet<&str> = STOPWORDS.iter().copied().collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();

    let push = |t: String, seen: &mut HashSet<String>, out: &mut Vec<String>| {
        if !seen.contains(&t) {
            seen.insert(t.clone());
            out.push(t);
        }
    };

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphabetic() {
            let start = i;
            i += 1;
            while i < chars.len() {
                let b = chars[i];
                if b.is_ascii_alphanumeric() || b == '_' || b == '-' { i += 1; } else { break; }
            }
            let raw: String = chars[start..i].iter().collect();
            let t: String = raw.to_ascii_lowercase();
            if t.len() < 3 || stops.contains(t.as_str()) { continue; }
            let pushed: String = if t.len() > 5 && t.ends_with("ing") {
                t[..t.len()-3].to_string()
            } else if t.len() > 5 && t.ends_with("ies") {
                let mut s = t[..t.len()-3].to_string(); s.push('y'); s
            } else if t.len() > 5 && t.ends_with("es") {
                t[..t.len()-2].to_string()
            } else if t.len() > 4 && t.ends_with('s') && !t.ends_with("ss") {
                t[..t.len()-1].to_string()
            } else {
                t
            };
            push(pushed, &mut seen, &mut out);
        } else if is_hangul(c) {
            let start = i;
            i += 1;
            while i < chars.len() && is_hangul(chars[i]) { i += 1; }
            let ko: String = chars[start..i].iter().collect();
            if ko.chars().count() < 2 { continue; }
            // Emit the Korean token itself.
            push(ko.clone(), &mut seen, &mut out);
            // Synonym expansion: add English equivalents.
            if let Some(&(_, engs)) = KO_EN_SYNONYMS.iter().find(|(k, _)| *k == ko.as_str()) {
                for eng in engs {
                    push((*eng).to_string(), &mut seen, &mut out);
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Tokenizer regression suite.
    //!
    //! These tests lock in the **current** observable behavior of the Rust
    //! `tokenize()` function on a set of anchor inputs that also appear in
    //! `learning/tests/test_tokenize.py`. The two test suites do NOT assert
    //! that the Rust and Python tokenizers produce identical token sets —
    //! they intentionally differ (Python emits compound + parts; Rust emits
    //! suffix-stripped compound only) — but both pin their respective sides
    //! so unintentional drift gets caught.
    //!
    //! When you legitimately change tokenize() behavior:
    //!   1. Run `cargo test -p we-forgectl tokenizer` and update the expected
    //!      vectors here.
    //!   2. Run `python3 -m unittest learning.tests.test_tokenize` and update
    //!      the parallel Python expectations.
    //!   3. Rebuild the ECC index: `python3 learning/build_ecc_index.py`.
    //!   4. Re-run `we-forgectl skill-regressions` to confirm no anchor case
    //!      ranking regressed.
    use super::tokenize;
    fn t(s: &str) -> Vec<String> { tokenize(s) }

    #[test]
    fn tokenizer_empty_string() {
        assert!(t("").is_empty());
    }

    #[test]
    fn tokenizer_length_floor_three() {
        // Tokens of length <3 are dropped; >=3 survive (and the >5-with-s
        // suffix rule strips trailing 's').
        let r = t("a bc abc abcd abcde");
        assert_eq!(r, vec!["abc".to_string(), "abcd".to_string(), "abcde".to_string()]);
    }

    #[test]
    fn tokenizer_suffix_stripping() {
        // len>5 + ending: ing→strip 3, ies→strip 3 + 'y', es→strip 2,
        // s (not ss) with len>4 → strip 1. Lowercased + deduped.
        let r = t("deploying deploys runs running stripped");
        // deploying (9, ing) → deploy; deploys (7, s, !ss) → deploy (dup, skip)
        // runs (4, s, !ss) → strip-s requires len>4 (strict) → "runs" stays raw
        // running (7, ing) → runn; stripped (8, s+ed... ends with 'd', no rule) → stripped
        assert_eq!(r, vec![
            "deploy".to_string(),
            "runs".to_string(),
            "runn".to_string(),
            "stripped".to_string(),
        ]);
    }

    #[test]
    fn tokenizer_treats_hyphen_as_part_of_token() {
        // Unlike Python (which also splits on `-`/`_` into parts), Rust
        // emits one concatenated token then suffix-strips the trailing 's'.
        // This asymmetry is reconciled downstream by split_slug_parts() in
        // the scoring path.
        let r = t("kotlin-coroutines-flows");
        assert_eq!(r, vec!["kotlin-coroutines-flow".to_string()]);
    }

    #[test]
    fn tokenizer_stopwords_dropped() {
        // "the", "and", "for", "with" appear in STOPWORDS; "git" survives.
        let r = t("the git and the and for the");
        assert_eq!(r, vec!["git".to_string()]);
    }

    #[test]
    fn tokenizer_korean_solo_with_synonym_expand() {
        // Pure-Korean run ≥2 syllables → emit raw + KO_EN synonyms.
        // "배포" → ["배포", "deploy", "deployment", "release"]
        let r = t("배포");
        assert_eq!(r, vec![
            "배포".to_string(),
            "deploy".to_string(),
            "deployment".to_string(),
            "release".to_string(),
        ]);
    }

    #[test]
    fn tokenizer_korean_single_syllable_dropped() {
        // <2 syllables → not a token.
        let r = t("팟 앱 가");
        assert!(r.is_empty());
    }

    #[test]
    fn tokenizer_mixed_korean_english_anchor() {
        // From skill-suggest-regressions.json id=ko-postgres.
        // Both tokenization paths run; ordering is English first (text
        // order: PostgreSQL precedes Korean tokens).
        let r = t("PostgreSQL 쿼리 최적화 인덱스");
        assert_eq!(r, vec![
            "postgresql".to_string(),
            "쿼리".to_string(),
            "query".to_string(),
            "최적화".to_string(),
            "optimization".to_string(),
            "optimize".to_string(),
            "인덱스".to_string(),
            "index".to_string(),
        ]);
    }

    #[test]
    fn tokenizer_dedupe_within_one_pass() {
        // Repeated tokens (after stripping) collapse via the `seen` set.
        let r = t("query Query QUERY querys");
        // "query" (raw, len=5, no rule fires since len !>5), "querys" (6, s, !ss, len>4) → "query" (dup)
        assert_eq!(r, vec!["query".to_string()]);
    }
}

