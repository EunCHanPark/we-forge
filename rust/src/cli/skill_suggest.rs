use super::*;
use std::collections::{BTreeMap, HashSet};
use std::fs;

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

fn tokenize(text: &str) -> Vec<String> {
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

fn split_slug_parts(slug: &str) -> Vec<String> {
    slug.split(|c: char| c == '-' || c == '_')
        .filter(|p| p.len() >= 3)
        .map(|p| p.to_ascii_lowercase())
        .collect()
}

struct Ranked {
    namespaced_slug: String,
    slug: String,
    description: String,
    score: f64,
    overlap: Vec<String>,
}

/// Public wrapper around the private `rank()` for sibling modules
/// (e.g. `skill_regressions`) that want the top-1 result. Returns
/// (namespaced_slug, score) or None when nothing matches.
pub fn rank_top1(prompt: &str) -> Option<(String, f64)> {
    rank(prompt, 1, 0.0)
        .into_iter()
        .next()
        .map(|r| (r.namespaced_slug, r.score))
}

fn rank(prompt: &str, top_n: usize, min_score: f64) -> Vec<Ranked> {
    let path = paths::ecc_index_file();
    if !path.exists() { return vec![]; }
    let txt = match fs::read_to_string(&path) { Ok(s) => s, Err(_) => return vec![] };
    let idx: serde_json::Value = match serde_json::from_str(&txt) { Ok(v) => v, Err(_) => return vec![] };
    let skills = match idx.get("skills").and_then(|x| x.as_array()) {
        Some(a) => a, None => return vec![],
    };
    let idf = match idx.get("idf").and_then(|x| x.as_object()) {
        Some(o) => o, None => return vec![],
    };
    let prompt_tokens: HashSet<String> = tokenize(prompt).into_iter().collect();
    if prompt_tokens.is_empty() { return vec![]; }

    let threshold = if min_score <= 0.0 { 3.0 } else { min_score };
    let slug_boost = 3.0_f64;
    let prefix_boost = 4.0_f64;

    struct Entry { score: f64, namespaced: String, slug: String, desc: String, overlap: Vec<String> }
    let mut by_ns: BTreeMap<String, Entry> = BTreeMap::new();

    for s in skills {
        if !s.get("suggestable").and_then(|x| x.as_bool()).unwrap_or(false) { continue; }
        let skill_tokens: HashSet<String> = s.get("tokens").and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        if skill_tokens.is_empty() { continue; }
        let overlap: Vec<String> = prompt_tokens.intersection(&skill_tokens).cloned().collect();
        if overlap.is_empty() { continue; }
        let slug = s.get("slug").and_then(|x| x.as_str()).unwrap_or("").to_ascii_lowercase();
        let slug_first = slug.split_once('-').map(|(a, _)| a.to_string()).unwrap_or_else(|| slug.clone());
        let mut slug_token_set: HashSet<String> = split_slug_parts(&slug).into_iter().collect();
        // Also include name parts
        if let Some(name) = s.get("name").and_then(|x| x.as_str()) {
            for p in split_slug_parts(&name.to_ascii_lowercase()) {
                slug_token_set.insert(p);
            }
        }
        let mut score = 0.0_f64;
        for t in &overlap {
            let mut w = idf.get(t).and_then(|v| v.as_f64()).unwrap_or(0.0);
            if t == &slug_first { w *= prefix_boost; }
            else if slug_token_set.contains(t) { w *= slug_boost; }
            score += w;
        }
        if score < threshold { continue; }
        let ns = s.get("namespaced_slug").and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| slug.clone());
        if ns.is_empty() { continue; }
        let desc: String = s.get("description").and_then(|x| x.as_str()).unwrap_or("").chars().take(140).collect();
        let mut sorted_overlap = overlap.clone();
        sorted_overlap.sort();
        let entry = Entry { score, namespaced: ns.clone(), slug: slug.clone(), desc, overlap: sorted_overlap };
        match by_ns.get(&ns) {
            Some(prev) if prev.score >= score => {}
            _ => { by_ns.insert(ns, entry); }
        }
    }

    let mut all: Vec<Entry> = by_ns.into_values().collect();
    all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    all.into_iter().take(top_n).map(|e| Ranked {
        namespaced_slug: e.namespaced,
        slug: e.slug,
        description: e.desc,
        score: (e.score * 100.0).round() / 100.0,
        overlap: e.overlap,
    }).collect()
}

fn short_hash(s: &str) -> String {
    // Cheap non-crypto rolling hash → 8 hex chars (FNV-1a).
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h).chars().take(8).collect()
}

fn log_turn(prompt: &str, session_id: &str) {
    let _ = std::fs::create_dir_all(paths::we_forge_home());
    let rec = serde_json::json!({
        "ts": now_iso(),
        "session_id": session_id,
        "prompt_len": prompt.chars().count(),
    });
    let _ = std::fs::OpenOptions::new().create(true).append(true)
        .open(paths::turns_log())
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", rec)
        });
}

fn log_suggestion(prompt: &str, suggestions: &[Ranked], session_id: &str) {
    let _ = std::fs::create_dir_all(paths::we_forge_home());
    let suggested: Vec<&str> = suggestions.iter().map(|s| s.namespaced_slug.as_str()).collect();
    let scores: Vec<f64> = suggestions.iter().map(|s| s.score).collect();
    let preview: String = prompt.chars().take(120).collect();
    let rec = serde_json::json!({
        "ts": now_iso(),
        "session_id": session_id,
        "prompt_hash": short_hash(prompt),
        "prompt_preview": preview,
        "suggested": suggested,
        "scores": scores,
    });
    let _ = std::fs::OpenOptions::new().create(true).append(true)
        .open(paths::suggest_log())
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", rec)
        });
}

/// Extract Hangul runs from the prompt (≥2 syllables, deduped).
/// Unlike `tokenize`, this does NOT expand via synonym dict — we want
/// to see what Korean words appeared *raw* in the prompt.
fn extract_hangul_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if ('\u{AC00}'..='\u{D7A3}').contains(&chars[i]) {
            let start = i;
            i += 1;
            while i < chars.len() && ('\u{AC00}'..='\u{D7A3}').contains(&chars[i]) { i += 1; }
            let ko: String = chars[start..i].iter().collect();
            if ko.chars().count() >= 2 && !seen.contains(&ko) {
                seen.insert(ko.clone());
                out.push(ko);
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Append a candidate row when we see Korean tokens not covered by
/// `KO_EN_SYNONYMS`, especially when the top match scored weakly. The
/// resulting jsonl feeds `we-forgectl synonym-candidates` so a human
/// can grow the dictionary deliberately. Logging is best-effort and
/// silently swallowed on error — we never want learning telemetry to
/// fail a hook injection.
fn log_synonym_candidate(
    prompt: &str,
    unknown_korean: &[String],
    top_score: f64,
    session_id: &str,
) {
    if unknown_korean.is_empty() { return; }
    let _ = std::fs::create_dir_all(paths::we_forge_home());
    let preview: String = prompt.chars().take(140).collect();
    let rec = serde_json::json!({
        "ts": now_iso(),
        "session_id": session_id,
        "prompt_hash": short_hash(prompt),
        "prompt_preview": preview,
        "unknown_korean": unknown_korean,
        "top_score": top_score,
    });
    let _ = std::fs::OpenOptions::new().create(true).append(true)
        .open(paths::synonym_candidates_log())
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", rec)
        });
}

// -----------------------------------------------------------------------
// workflow_match — opt-in (cfg.workflow_suggest_enabled). Pattern-match
// the prompt to ECC multi-agent workflow skills (`/santa-method`,
// `/council`, `/multi-workflow`, `/gan-style-harness`, …). Returns a small
// ranked list of (slug, why). Conservative on purpose: precision >
// recall, so the injection stays useful rather than noisy.
// -----------------------------------------------------------------------
struct WfRule {
    slug: &'static str,   // namespaced ECC skill slug
    why:  &'static str,   // one-line rationale shown next to the recommendation
    // Each pattern is a list of substrings; ALL must appear (case-insensitive)
    // somewhere in the prompt for the rule to fire. ANY of the patterns can
    // trigger.
    any_of_all: &'static [&'static [&'static str]],
}

const WORKFLOW_RULES: &[WfRule] = &[
    // --- Convergence / consensus ----------------------------------------
    WfRule {
        slug: "everything-claude-code:santa-method",
        why:  "production-bound code / dual-reviewer convergence",
        any_of_all: &[
            &["production"], &["deploy"],
            &["push to main"], &["push to master"],
            &["release candidate"], &["before shipping"],
            &["before merging"], &["before merge"],
            &["compliance"], &["regulatory"],
            &["customer-facing"], &["pre-launch"],
            &["go live"], &["ready to ship"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:council",
        why:  "ambiguous tradeoff / multiple valid paths — convene 4-voice council",
        any_of_all: &[
            &["should i"], &["should we"],
            &["trade-off"], &["tradeoff"],
            &[" vs "], &[" or "],
            &["which", "better"], &["which", "choose"],
            &["which", "should"],
            &["pros and cons"], &["decide between"], &["decide on"],
            &["go/no-go"], &["go-no-go"],
            &["pick between"], &["choose between"],
            &["second opinion"], &["dissent"],
        ],
    },

    // --- Multi-phase delivery ------------------------------------------
    WfRule {
        slug: "everything-claude-code:multi-workflow",
        why:  "multi-phase feature build (research → plan → execute → review)",
        any_of_all: &[
            &["new feature"], &["implement", "across"],
            &["build out", "feature"], &["multi-file"],
            &["refactor", "across"], &["end-to-end implementation"],
            &["full implementation"], &["complete implementation"],
            &["from", "to deployment"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:gan-style-harness",
        why:  "long-running autonomous app build (generator/evaluator loop)",
        any_of_all: &[
            &["build", "app", "from"], &["from scratch"],
            &["prd"], &["from a one-liner"],
            &["autonomous", "build"], &["one-liner", "to"],
            &["scaffold", "entire"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:multi-frontend",
        why:  "frontend-focused multi-model workflow (UI/UX/animation)",
        any_of_all: &[
            &["frontend", "feature"], &["ui", "polish"],
            &["component library"], &["design system"],
            &["ux", "iterate"], &["pixel-perfect"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:multi-backend",
        why:  "backend-focused multi-model workflow (APIs/algorithms/data)",
        any_of_all: &[
            &["backend", "feature"], &["api", "design"],
            &["database", "schema"], &["service", "architecture"],
            &["microservice"], &["data pipeline"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:team-builder",
        why:  "ad-hoc parallel team across mixed domains (interactive picker)",
        any_of_all: &[
            &["pick agents"], &["compose team"], &["choose agents"],
            &["parallel team"], &["dispatch", "agents"],
            &["agent team"], &["which agents"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:dmux-workflows",
        why:  "multi-agent orchestration in tmux/dmux/cmux panes (multiple OS processes)",
        any_of_all: &[
            // Distinctive single tokens — safe alone
            &["dmux"], &["cmux"],
            // tmux is too generic — require a coordination cue alongside it
            &["tmux", "claude"], &["tmux", "agent"], &["tmux", "pane"],
            &["tmux", "session", "parallel"],
            // Parallelism + agent/claude/instance signals
            &["parallel", "agent"], &["parallel", "claude"], &["parallel", "instance"],
            &["multiple claude"], &["multiple", "instances"],
            &["run", "agents", "parallel"], &["run", "claude", "parallel"],
            &["agents", "in parallel"], &["claudes", "in parallel"],
            // Work splitting / coordination patterns
            &["split work"], &["divide and conquer"],
            &["pane", "agent"], &["pane", "claude"],
            &["fan out", "agent"], &["fan-out", "agent"],
            &["claude-teams"],
        ],
    },

    // --- Review / audit -------------------------------------------------
    WfRule {
        slug: "everything-claude-code:review-pr",
        why:  "PR review via specialized review agents",
        any_of_all: &[
            &["review pr"], &["review", "pull request"],
            &["pr review"], &["pr #"], &["review my pr"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:code-review",
        why:  "comprehensive code review (uncommitted changes or PR)",
        any_of_all: &[
            &["code review"], &["review", "changes"],
            &["review my code"], &["review this code"],
            &["lgtm"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:security-review",
        why:  "security-focused review pass",
        any_of_all: &[
            &["security review"], &["audit", "security"],
            &["vulnerab"], &["threat model"],
            &["secure code"], &["security audit"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:harness-audit",
        why:  "deterministic harness audit + prioritized scorecard",
        any_of_all: &[
            &["audit my setup"], &["audit", "harness"],
            &["harness health"], &["harness audit"],
            &["audit", "config"],
        ],
    },

    // --- Planning / PRD -------------------------------------------------
    WfRule {
        slug: "everything-claude-code:prp-plan",
        why:  "feature implementation plan with codebase analysis",
        any_of_all: &[
            &["implementation plan"], &["plan", "implementation"],
            &["plan", "feature"], &["feature plan"],
            &["plan", "implement"], &["plan", "refactor"],
            &["break down", "task"], &["roadmap for"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:prp-prd",
        why:  "interactive PRD generator (problem-first, hypothesis-driven)",
        any_of_all: &[
            &["prd"], &["product spec"], &["product requirements"],
            &["product brief"], &["write a spec"],
            &["draft a spec"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:plan",
        why:  "step-by-step implementation plan (wait for CONFIRM)",
        any_of_all: &[
            &["plan", "before"], &["plan first"],
            &["plan this", "out"],
            &["explain", "approach"],
        ],
    },

    // --- Testing / verification ----------------------------------------
    WfRule {
        slug: "everything-claude-code:tdd-workflow",
        why:  "test-first development (write tests, then implement)",
        any_of_all: &[
            &["tdd"], &["test-driven"], &["test driven"],
            &["tests first"], &["test first"],
            &["write tests before"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:e2e-testing",
        why:  "end-to-end test setup and runner",
        any_of_all: &[
            &["e2e test"], &["end-to-end test"], &["end to end test"],
            &["playwright"], &["cypress"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:verification-loop",
        why:  "structured verification + remediation loop",
        any_of_all: &[
            &["verification loop"], &["verify", "rigorous"],
            &["validation loop"], &["verify the implementation"],
            &["pass all checks"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:test-coverage",
        why:  "coverage analysis + missing-test generation",
        any_of_all: &[
            &["test coverage"], &["coverage", "gap"],
            &["coverage report"], &["missing tests"],
        ],
    },

    // --- Cleanup / safety ----------------------------------------------
    WfRule {
        slug: "everything-claude-code:refactor-clean",
        why:  "dead-code cleanup with per-step verification",
        any_of_all: &[
            &["dead code"], &["unused code"],
            &["clean up", "dead"], &["remove unused"],
            &["dead-code"], &["dead .md"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:safety-guard",
        why:  "destructive-operation gate before agent action",
        any_of_all: &[
            &["rm -rf"], &["drop table"],
            &["delete the"], &["force push"],
            &["before i delete"], &["destructive"],
        ],
    },

    // --- Meta / tooling -------------------------------------------------
    WfRule {
        slug: "everything-claude-code:prompt-optimizer",
        why:  "rewrite user prompt for better ECC routing (advisory only)",
        any_of_all: &[
            &["optimize", "prompt"], &["improve", "prompt"],
            &["better prompt"], &["prompt engineering"],
            &["rewrite this prompt"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:model-route",
        why:  "recommend model tier (Haiku vs Sonnet vs Opus) for this task",
        any_of_all: &[
            &["which model"], &["haiku", "sonnet"],
            &["sonnet", "opus"], &["model tier"],
            &["model selection"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:agent-eval",
        why:  "head-to-head comparison of coding agents on custom tasks",
        any_of_all: &[
            &["compare agents"], &["benchmark agents"],
            &["claude", "aider"], &["claude", "codex"],
            &["aider", "codex"], &["agent benchmark"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:code-tour",
        why:  "persona-targeted CodeTour walkthrough (.tour files)",
        any_of_all: &[
            &["onboarding tour"], &["code tour"],
            &["walkthrough", "codebase"], &["walkthrough", "code"],
            &["walk through", "code"], &["explain how", "works"],
            &["architecture walkthrough"], &["tour", "junior"],
            &["walkthrough", "junior"],
        ],
    },
    WfRule {
        slug: "everything-claude-code:codebase-onboarding",
        why:  "unfamiliar-codebase analysis → onboarding guide + CLAUDE.md starter",
        any_of_all: &[
            &["new repo"], &["unfamiliar codebase"],
            &["first time", "repo"], &["joining", "project"],
            &["onboard me"],
        ],
    },
];

fn workflow_match(prompt: &str, max_n: usize) -> Vec<(&'static str, &'static str)> {
    let lc = prompt.to_ascii_lowercase();
    let mut hits: Vec<(&'static str, &'static str)> = Vec::new();
    for rule in WORKFLOW_RULES {
        let fired = rule.any_of_all.iter().any(|reqs|
            reqs.iter().all(|needle| lc.contains(&needle.to_ascii_lowercase()))
        );
        if fired {
            if !hits.iter().any(|(s, _)| *s == rule.slug) {
                hits.push((rule.slug, rule.why));
                if hits.len() >= max_n { break; }
            }
        }
    }
    hits
}

pub fn run(prompt: &str, top_n: usize, inject: bool, log: bool, session_id: &str) -> Result<()> {
    let prompt = prompt.trim();
    if prompt.is_empty() { return Ok(()); }
    // Quick-path: silent skip on trivial prompts.
    if prompt.chars().count() < 15 {
        if log { log_turn(prompt, session_id); }
        return Ok(());
    }
    let suggestions = rank(prompt, top_n.max(1), 0.0);

    // Opt-in: ECC multi-agent workflow recommendations (Level-3 patterns).
    // Gated by config.workflow_suggest_enabled (default off).
    let cfg = config::with_env_overrides(config::load());
    let workflows = if cfg.workflow_suggest_enabled {
        workflow_match(prompt, 3)
    } else {
        Vec::new()
    };

    if log {
        log_turn(prompt, session_id);
        if !suggestions.is_empty() {
            log_suggestion(prompt, &suggestions, session_id);
        }
        // Synonym learning loop: when the prompt has Korean tokens we
        // don't have synonym coverage for AND the top suggestion is
        // weak, log the unknown words so a human can grow the dict.
        // Threshold: top_score < 5.0 catches "barely matched at all"
        // cases (the canary-watch baseline before patches was 5.12).
        let hangul = extract_hangul_tokens(prompt);
        if !hangul.is_empty() {
            let unknown: Vec<String> = hangul
                .into_iter()
                .filter(|ko| !KO_EN_SYNONYMS.iter().any(|(k, _)| *k == ko.as_str()))
                .collect();
            let top_score = suggestions.first().map(|s| s.score).unwrap_or(0.0);
            if !unknown.is_empty() && top_score < 5.0 {
                log_synonym_candidate(prompt, &unknown, top_score, session_id);
            }
        }
    }
    if inject {
        let mut lines = vec!["<system-reminder>".to_string()];
        if suggestions.is_empty() && workflows.is_empty() {
            // No ECC match — inject explicit notice so Claude announces it.
            lines.push("skill-suggest: 이 프롬프트에 매칭되는 ECC 스킬 없음.".to_string());
            lines.push("작업 시작 전 반드시 한 줄 알림: \"ECC 매칭 없음 — 일반 진행\"".to_string());
        } else {
            if !suggestions.is_empty() {
                lines.push(format!("we-forge skill-suggest: top {} ECC skills matched (IDF-weighted):", suggestions.len()));
                lines.push("".to_string());
                for (i, s) in suggestions.iter().enumerate() {
                    lines.push(format!("{}. `{}` (score {}) — {}", i + 1, s.namespaced_slug, s.score, s.description));
                }
                lines.push("".to_string());
                lines.push("매칭 스킬이 있으면: (1) \"💡 ECC: `<name>` 사용합니다.\" 한 줄 알림 후 (2) Skill() 호출 → 작업 진행. 해당 없으면: \"ECC 후보 있으나 무관 (<names>) — 일반 진행\" 한 줄 알림 후 작업.".to_string());
            }
            if !workflows.is_empty() {
                if !suggestions.is_empty() { lines.push("".to_string()); }
                lines.push("Multi-agent workflow recommendations (opt-in, prompt-pattern matched):".to_string());
                for (slug, why) in &workflows {
                    lines.push(format!("- `{}` — {}", slug, why));
                }
                lines.push("Invoke via the Skill tool only if the user's task fits this workflow shape; otherwise ignore.".to_string());
            }
        }
        lines.push("</system-reminder>".to_string());
        println!("{}", lines.join("\n"));
        return Ok(());
    }

    if suggestions.is_empty() && workflows.is_empty() { return Ok(()); }

    println!("skill-suggest: top {} for prompt ({} chars)", suggestions.len(), prompt.chars().count());
    for (i, s) in suggestions.iter().enumerate() {
        let desc_short: String = s.description.chars().take(70).collect();
        println!("  {}. {:55} score={:5}  {}", i + 1, s.namespaced_slug, s.score, desc_short);
        let ov: Vec<String> = s.overlap.iter().take(8).cloned().collect();
        println!("     overlap: {}", ov.join(", "));
        let _ = &s.slug;
    }
    if !workflows.is_empty() {
        println!("workflow-suggest: {} match{}", workflows.len(), if workflows.len() == 1 { "" } else { "es" });
        for (slug, why) in &workflows {
            println!("  - {:55} {}", slug, why);
        }
    }
    Ok(())
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

