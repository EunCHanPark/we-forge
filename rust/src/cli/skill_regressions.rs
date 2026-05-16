use super::*;
use super::skill_suggest::rank_top1;
use std::fs;
use std::path::PathBuf;

/// Locate the regressions fixture. Try (in order):
///   1. $WE_FORGE_REGRESSIONS_FILE      (explicit override for testing)
///   2. ./learning/skill-suggest-regressions.json   (when run from repo root)
///   3. ~/.claude/learning/skill-suggest-regressions.json   (installed copy)
fn locate_fixture() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("WE_FORGE_REGRESSIONS_FILE") {
        let pb = PathBuf::from(p);
        if pb.is_file() { return Some(pb); }
    }
    let cwd_path = PathBuf::from("learning/skill-suggest-regressions.json");
    if cwd_path.is_file() { return Some(cwd_path); }
    let installed = paths::claude_home().join("learning/skill-suggest-regressions.json");
    if installed.is_file() { return Some(installed); }
    None
}

pub fn run(verbose: bool) -> Result<()> {
    let path = match locate_fixture() {
        Some(p) => p,
        None => {
            eprintln!("skill-regressions: fixture not found.");
            eprintln!("  searched:");
            eprintln!("    $WE_FORGE_REGRESSIONS_FILE");
            eprintln!("    ./learning/skill-suggest-regressions.json");
            eprintln!("    {}", paths::claude_home().join("learning/skill-suggest-regressions.json").display());
            std::process::exit(2);
        }
    };
    let text = fs::read_to_string(&path)?;
    let doc: serde_json::Value = serde_json::from_str(&text)?;
    let cases = match doc.get("cases").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => {
            eprintln!("skill-regressions: fixture {} has no `cases` array", path.display());
            std::process::exit(2);
        }
    };

    println!("skill-regressions: fixture {}", path.display());
    println!("                   {} cases", cases.len());
    println!();

    let mut failures: Vec<String> = Vec::new();
    let mut passed = 0_usize;

    for (i, case) in cases.iter().enumerate() {
        let id = case.get("id").and_then(|x| x.as_str()).unwrap_or("(no id)");
        let prompt = case.get("prompt").and_then(|x| x.as_str()).unwrap_or("");
        let expect = case.get("expect_top").and_then(|x| x.as_str()).unwrap_or("");
        let min_score = case.get("min_score").and_then(|x| x.as_f64()).unwrap_or(0.0);

        let (got_slug, got_score) = match rank_top1(prompt) {
            Some((s, sc)) => (s, sc),
            None => (String::from("(no match)"), 0.0),
        };

        let top_ok = got_slug == expect;
        let score_ok = got_score >= min_score;
        let pass = top_ok && score_ok;

        let status = if pass { "PASS" } else { "FAIL" };
        let baseline = case.get("baseline_score").and_then(|x| x.as_f64()).unwrap_or(-1.0);
        let baseline_str = if baseline >= 0.0 { format!("{:.2}", baseline) } else { "—".to_string() };

        if pass {
            passed += 1;
            if verbose {
                println!("  [{:>4}]  {:>2}. {:<30}  got={:.2} (≥{:.1}) ✓", status, i + 1, id, got_score, min_score);
            }
        } else {
            let mut reason = Vec::new();
            if !top_ok { reason.push(format!("top != {} (got {})", expect, got_slug)); }
            if !score_ok { reason.push(format!("score {:.2} < min_score {:.1}", got_score, min_score)); }
            println!("  [{:>4}]  {:>2}. {:<30}", status, i + 1, id);
            println!("            prompt: {}", prompt);
            println!("            expect: {} (baseline {})", expect, baseline_str);
            println!("            got:    {} (score {:.2}, floor {:.1})", got_slug, got_score, min_score);
            println!("            why:    {}", reason.join("; "));
            failures.push(id.to_string());
        }
    }

    println!();
    println!("skill-regressions: {} passed, {} failed", passed, failures.len());
    if !failures.is_empty() {
        println!("                   failed cases: {}", failures.join(", "));
        std::process::exit(1);
    }
    Ok(())
}
