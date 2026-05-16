use super::*;
use std::collections::BTreeMap;
use std::fs;

pub fn run(top: usize, hours: i64) -> Result<()> {
    let cutoff_iso = if hours > 0 {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours);
        cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    } else {
        String::new()  // empty string sorts < all real timestamps
    };

    let path = paths::synonym_candidates_log();
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            println!("synonym-candidates: no entries yet ({} not found)", path.display());
            println!("                    skill-suggest writes here when a Korean prompt");
            println!("                    scores < 5.0 with at least one unknown Korean token.");
            return Ok(());
        }
    };

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_rows = 0_usize;
    let mut considered_rows = 0_usize;
    let mut last_seen: BTreeMap<String, String> = BTreeMap::new();
    let mut sample: BTreeMap<String, String> = BTreeMap::new();

    for line in text.lines() {
        if line.trim().is_empty() { continue; }
        total_rows += 1;
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v, Err(_) => continue,
        };
        let ts = v.get("ts").and_then(|x| x.as_str()).unwrap_or("");
        if !cutoff_iso.is_empty() && ts < cutoff_iso.as_str() { continue; }
        considered_rows += 1;
        let preview = v.get("prompt_preview").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if let Some(arr) = v.get("unknown_korean").and_then(|x| x.as_array()) {
            for tok in arr {
                if let Some(s) = tok.as_str() {
                    *counts.entry(s.to_string()).or_insert(0) += 1;
                    last_seen.insert(s.to_string(), ts.to_string());
                    sample.entry(s.to_string()).or_insert_with(|| preview.clone());
                }
            }
        }
    }

    if counts.is_empty() {
        let window = if hours > 0 { format!("last {} h", hours) } else { "all time".to_string() };
        println!("synonym-candidates: no unknown Korean tokens in window ({})", window);
        println!("                    log rows scanned: {}, in window: {}", total_rows, considered_rows);
        return Ok(());
    }

    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let window = if hours > 0 { format!("last {} h", hours) } else { "all time".to_string() };
    println!("synonym-candidates: top {} unknown Korean tokens ({})",
             ranked.len().min(top), window);
    println!("                    {} rows total / {} in window", total_rows, considered_rows);
    println!();
    for (i, (tok, n)) in ranked.iter().take(top).enumerate() {
        let last = last_seen.get(tok).cloned().unwrap_or_default();
        let sample_str = sample.get(tok).cloned().unwrap_or_default();
        let sample_short: String = sample_str.chars().take(60).collect();
        println!("  {:>2}. {:8}  ×{:<4}  last={}  e.g. {}",
                 i + 1, tok, n,
                 last.chars().take(19).collect::<String>(),
                 sample_short);
    }
    println!();
    println!("To add a mapping, edit BOTH:");
    println!("  learning/build_ecc_index.py  _KO_EN_SYNONYMS");
    println!("  rust/src/cli.rs              KO_EN_SYNONYMS  (then `cargo build --release` + install)");
    Ok(())
}
