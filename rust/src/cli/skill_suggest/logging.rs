//! Append-only JSONL logging — turns, suggestions, synonym candidates.
//! All writes are best-effort; failures are silently swallowed so a
//! learning telemetry hiccup never blocks a hook injection.

use crate::core::{now_iso, paths};
use super::rank::Ranked;

pub(super) fn short_hash(s: &str) -> String {
    // Cheap non-crypto rolling hash → 8 hex chars (FNV-1a).
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h).chars().take(8).collect()
}

pub(super) fn log_turn(prompt: &str, session_id: &str) {
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

pub(super) fn log_suggestion(prompt: &str, suggestions: &[Ranked], session_id: &str) {
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
