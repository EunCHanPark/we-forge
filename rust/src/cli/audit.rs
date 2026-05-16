use super::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    let truncated: String = trimmed.chars().take(60).collect();
    let final_s = truncated.trim_matches('-').to_string();
    if final_s.is_empty() { "pattern".into() } else { final_s }
}

pub fn run(top_n: usize) -> Result<()> {
    let learn = paths::learning_data_dir();
    let pat_path = learn.join("patterns.jsonl");
    let led_path = learn.join("ledger.jsonl");
    let rej_path = learn.join("rejected.txt");
    if !pat_path.exists() {
        println!("  WARN no patterns yet: {}", pat_path.display());
        return Ok(());
    }
    let rejected: HashSet<String> = if rej_path.exists() {
        fs::read_to_string(&rej_path).unwrap_or_default()
            .lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
    } else {
        HashSet::new()
    };

    let mut patterns: Vec<serde_json::Value> = Vec::new();
    if let Ok(text) = fs::read_to_string(&pat_path) {
        for line in text.lines() {
            if line.trim().is_empty() { continue; }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                patterns.push(v);
            }
        }
    }

    let mut slug_to_decisions: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    if led_path.exists() {
        if let Ok(text) = fs::read_to_string(&led_path) {
            for line in text.lines() {
                if line.trim().is_empty() { continue; }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(s) = v.get("slug").and_then(|x| x.as_str()) {
                        slug_to_decisions.entry(s.to_string()).or_default().push(v);
                    }
                }
            }
        }
    }

    struct Row { count: i64, verdict: String, pattern: String, reason: String }
    let mut rows: Vec<Row> = Vec::new();
    for p in &patterns {
        let cnt = p.get("count").and_then(|x| x.as_i64()).unwrap_or(0);
        let sids = p.get("sample_session_ids").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);
        if cnt < 3 || sids < 3 { continue; }
        let pat = p.get("pattern").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let slug = slugify(&pat);
        let (verdict, reason) = if let Some(decs) = slug_to_decisions.get(&slug) {
            let last = decs.last().unwrap();
            let v = last.get("decision").and_then(|x| x.as_str()).unwrap_or("?").to_string();
            let r: String = last.get("reason").and_then(|x| x.as_str()).unwrap_or("").chars().take(60).collect();
            (v, r)
        } else if rejected.contains(&pat) {
            ("REJECTED".into(), "in rejected.txt (skipped from queue)".into())
        } else {
            ("no-ledger".into(), "(never reached ledger — investigate)".into())
        };
        let pat_short: String = pat.chars().take(60).collect();
        rows.push(Row { count: cnt, verdict, pattern: pat_short, reason });
    }
    rows.sort_by(|a, b| b.count.cmp(&a.count));

    println!("==> audit: top {top_n} patterns (count >= 3, distinct sessions >= 3)");
    println!("{:>6}  {:<12}  {:<60}  reason", "count", "verdict", "pattern");
    println!("{}", "-".repeat(110));
    for r in rows.iter().take(top_n) {
        println!("{:>6}  {:<12}  {:<60}  {}", r.count, r.verdict, r.pattern, r.reason);
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for r in &rows { *counts.entry(r.verdict.clone()).or_insert(0) += 1; }
    println!();
    println!("verdict breakdown (qualifying patterns): {:?}", counts);
    println!();
    println!("Interpretation:");
    println!("  - High count + ECC_MATCH: verify the matched skill genuinely fits.");
    println!("  - High count + DROP:      verify the primitive filter wasn't over-eager.");
    println!("  - High count + no-ledger: pipeline gap — should have been processed.");
    Ok(())
}
