//! skill-suggest subcommand — match user prompt against ECC marketplace
//! skills (BM25-lite) and emit an injection or human-readable rank.
//!
//! Split into focused submodules on 2026-05-16 (was a 752-LOC single file):
//!   tokenize           — STOPWORDS, KO_EN_SYNONYMS, the tokenize() fn
//!   rank               — IDF-weighted scoring + rank_top1 public helper
//!   logging            — turns / suggestions / synonym-candidate JSONL writes
//!   synonym_telemetry  — Korean-token extraction for dictionary growth
//!   workflow           — opt-in multi-agent workflow recommendations
//!
//! Only `run()`, `rank_top1`, and `KO_EN_SYNONYMS` are exposed outside the
//! module; everything else is `pub(super)` or private.

use super::*;

mod tokenize;
mod rank;
mod logging;
mod synonym_telemetry;
mod workflow;

// Re-exports consumed by sibling cli/ modules.
pub use rank::rank_top1;
pub use tokenize::KO_EN_SYNONYMS;

pub fn run(prompt: &str, top_n: usize, inject: bool, log: bool, session_id: &str) -> Result<()> {
    let prompt = prompt.trim();
    if prompt.is_empty() { return Ok(()); }
    // Quick-path: silent skip on trivial prompts.
    if prompt.chars().count() < 15 {
        if log { logging::log_turn(prompt, session_id); }
        return Ok(());
    }
    let suggestions = rank::rank(prompt, top_n.max(1), 0.0);

    // Opt-in: ECC multi-agent workflow recommendations (Level-3 patterns).
    // Gated by config.workflow_suggest_enabled (default off).
    let cfg = config::with_env_overrides(config::load());
    let workflows = if cfg.workflow_suggest_enabled {
        workflow::workflow_match(prompt, 3)
    } else {
        Vec::new()
    };

    if log {
        logging::log_turn(prompt, session_id);
        if !suggestions.is_empty() {
            logging::log_suggestion(prompt, &suggestions, session_id);
        }
        // Synonym learning loop: when the prompt has Korean tokens we
        // don't have synonym coverage for AND the top suggestion is
        // weak, log the unknown words so a human can grow the dict.
        // Threshold: top_score < 5.0 catches "barely matched at all"
        // cases (the canary-watch baseline before patches was 5.12).
        let hangul = synonym_telemetry::extract_hangul_tokens(prompt);
        if !hangul.is_empty() {
            let unknown: Vec<String> = hangul
                .into_iter()
                .filter(|ko| !KO_EN_SYNONYMS.iter().any(|(k, _)| *k == ko.as_str()))
                .collect();
            let top_score = suggestions.first().map(|s| s.score).unwrap_or(0.0);
            if !unknown.is_empty() && top_score < 5.0 {
                synonym_telemetry::log_synonym_candidate(prompt, &unknown, top_score, session_id);
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
