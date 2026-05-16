use super::*;
use std::collections::{BTreeMap, HashSet};
use std::fs;

pub fn run(window_hours: i64) -> Result<()> {
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(window_hours.max(0));
    let cutoff_iso = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let mut suggestions: Vec<serde_json::Value> = Vec::new();
    let sug_path = paths::suggest_log();
    if let Ok(text) = fs::read_to_string(&sug_path) {
        for line in text.lines() {
            if line.trim().is_empty() { continue; }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if v.get("ts").and_then(|x| x.as_str()).unwrap_or("") >= cutoff_iso.as_str() {
                    suggestions.push(v);
                }
            }
        }
    }

    let mut invoked: HashSet<String> = HashSet::new();
    let trace_path = paths::ecc_trace_file();
    if let Ok(text) = fs::read_to_string(&trace_path) {
        for line in text.lines() {
            if line.trim().is_empty() { continue; }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if v.get("ts").and_then(|x| x.as_str()).unwrap_or("") >= cutoff_iso.as_str() {
                    if let Some(sk) = v.get("skill").and_then(|x| x.as_str()) {
                        let sk = sk.trim().to_string();
                        if sk.is_empty() { continue; }
                        invoked.insert(sk.clone());
                        if let Some((_, after)) = sk.split_once(':') {
                            invoked.insert(after.to_string());
                        }
                    }
                }
            }
        }
    }

    let mut hits = 0_usize;
    let mut miss_counts: BTreeMap<String, usize> = BTreeMap::new();
    for sg in &suggestions {
        let suggested = match sg.get("suggested").and_then(|x| x.as_array()) {
            Some(a) if !a.is_empty() => a,
            _ => continue,
        };
        let mut any_hit = false;
        for ns_v in suggested {
            let ns = match ns_v.as_str() { Some(s) => s, None => continue };
            let bare = ns.split_once(':').map(|(_, a)| a).unwrap_or(ns);
            if invoked.contains(ns) || invoked.contains(bare) {
                any_hit = true;
                break;
            }
        }
        if any_hit {
            hits += 1;
        } else {
            for ns_v in suggested {
                if let Some(ns) = ns_v.as_str() {
                    *miss_counts.entry(ns.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    let n_sugg = suggestions.len();
    let rate = if n_sugg > 0 { hits as f64 / n_sugg as f64 * 100.0 } else { 0.0 };

    println!("==> skill-suggest hit rate (last {window_hours}h)");
    println!("  suggestions:  {n_sugg}");
    println!("  hits:         {hits}");
    println!("  hit rate:     {:.1}%", rate);
    if !miss_counts.is_empty() {
        println!();
        println!("  top misses (suggested but never invoked):");
        let mut top: Vec<(String, usize)> = miss_counts.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        for (slug, n) in top.iter().take(5) {
            println!("    {:3}  {}", n, slug);
        }
    }
    Ok(())
}
