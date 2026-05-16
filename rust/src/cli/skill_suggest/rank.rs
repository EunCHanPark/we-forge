//! IDF-weighted matching: turn a tokenized prompt into a ranked list of
//! ECC marketplace skills. Slug-part boosts (3x / 4x for prefix) preserve
//! the bias toward exact-name matches that hand-tuning relied on.

use crate::core::paths;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use super::tokenize::tokenize;

fn split_slug_parts(slug: &str) -> Vec<String> {
    slug.split(|c: char| c == '-' || c == '_')
        .filter(|p| p.len() >= 3)
        .map(|p| p.to_ascii_lowercase())
        .collect()
}

pub(super) struct Ranked {
    pub(super) namespaced_slug: String,
    pub(super) slug: String,
    pub(super) description: String,
    pub(super) score: f64,
    pub(super) overlap: Vec<String>,
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

pub(super) fn rank(prompt: &str, top_n: usize, min_score: f64) -> Vec<Ranked> {
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
