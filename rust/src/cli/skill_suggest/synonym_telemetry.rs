//! Korean-token telemetry — feeds `we-forgectl synonym-candidates` so the
//! ko↔en dictionary in `tokenize` can grow deliberately when real prompts
//! contain Korean we have no synonym coverage for.

use crate::core::{now_iso, paths};
use super::logging::short_hash;
use std::collections::HashSet;

/// Extract Hangul runs from the prompt (≥2 syllables, deduped).
/// Unlike `tokenize`, this does NOT expand via synonym dict — we want
/// to see what Korean words appeared *raw* in the prompt.
pub(super) fn extract_hangul_tokens(text: &str) -> Vec<String> {
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
pub(super) fn log_synonym_candidate(
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
