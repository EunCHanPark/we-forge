use super::*;
use std::collections::BTreeMap;
use std::fs;

pub fn run(score_threshold: i64) -> Result<()> {
    let led_path = paths::learning_data_dir().join("ledger.jsonl");
    if !led_path.exists() {
        println!("  WARN no ledger: {}", led_path.display());
        return Ok(());
    }
    // Score key: i64 score, or i64::MIN for "unknown"
    let mut score_dist: BTreeMap<i64, usize> = BTreeMap::new();
    let mut unknown: usize = 0;
    let mut skill_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut flagged: Vec<serde_json::Value> = Vec::new();

    if let Ok(text) = fs::read_to_string(&led_path) {
        for line in text.lines() {
            if line.trim().is_empty() { continue; }
            let d: serde_json::Value = match serde_json::from_str(line) {
                Ok(v)  => v,
                Err(_) => continue,
            };
            if d.get("decision").and_then(|x| x.as_str()) != Some("ECC_MATCH") { continue; }
            match d.get("match_score").and_then(|x| x.as_i64()) {
                Some(s) => {
                    *score_dist.entry(s).or_insert(0) += 1;
                    let skill = d.get("ecc_skill").and_then(|x| x.as_str()).unwrap_or("?").to_string();
                    *skill_counts.entry(skill).or_insert(0) += 1;
                    if s < score_threshold { flagged.push(d); }
                }
                None => {
                    unknown += 1;
                }
            }
        }
    }

    println!("==> ECC_MATCH match_score distribution (threshold={score_threshold})");
    if unknown > 0 {
        println!("  score {:<8} {:>4}", "unknown", unknown);
    }
    for (s, n) in &score_dist {
        let marker = if *s < score_threshold { " ⚠️" } else { "" };
        println!("  score {:<8} {:>4}{marker}", s, n);
    }
    println!();
    println!("==> top matched ECC skills (top 10)");
    let mut pairs: Vec<(String, usize)> = skill_counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1));
    for (skill, n) in pairs.iter().take(10) {
        println!("  {:>4}  {}", n, skill);
    }
    println!();
    if !flagged.is_empty() {
        println!("==> ⚠️  {} entries below score_threshold (REVISE downgrade candidates)", flagged.len());
        for d in flagged.iter().take(15) {
            let slug = d.get("slug").and_then(|x| x.as_str()).unwrap_or("");
            let score = d.get("match_score").and_then(|x| x.as_i64())
                .map(|s| s.to_string()).unwrap_or_else(|| "?".into());
            let ecc_skill = d.get("ecc_skill").and_then(|x| x.as_str()).unwrap_or("?");
            println!("  {:<35} score={} ecc_skill={}", slug, score, ecc_skill);
        }
    } else {
        println!("==> all ECC_MATCH entries score >= {score_threshold} ✓");
    }
    Ok(())
}
