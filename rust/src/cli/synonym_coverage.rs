use super::*;
use super::skill_suggest::KO_EN_SYNONYMS;
use std::collections::{BTreeMap, HashSet};
use std::fs;

pub fn run(top: usize, min_skills: usize) -> Result<()> {
    let path = paths::ecc_index_file();
    let txt = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            println!("synonym-coverage: ecc-index.json not found at {}", path.display());
            println!("                  rebuild with: python3 learning/build_ecc_index.py");
            return Ok(());
        }
    };
    let idx: serde_json::Value = match serde_json::from_str(&txt) {
        Ok(v) => v,
        Err(e) => {
            println!("synonym-coverage: failed to parse index: {}", e);
            return Ok(());
        }
    };

    // Build a set of English tokens already mapped from some Korean key.
    // Skill descriptions in the index keep their original plural / -ing
    // forms because the indexer does NOT suffix-strip. We over-generate
    // covered variants so "patterns" (skill text) counts as covered when
    // the dict maps 패턴 → "pattern" (singular). Over-generation is
    // intentional: a few false negatives in the report are better than
    // many false positives.
    let mut covered: HashSet<String> = HashSet::new();
    for (_ko, engs) in KO_EN_SYNONYMS.iter() {
        for e in *engs {
            covered.insert((*e).to_string());
            covered.insert(format!("{}s", e));        // pattern → patterns
            covered.insert(format!("{}ing", e));      // monitor → monitoring
            if e.ends_with("y") && e.len() > 1 {
                covered.insert(format!("{}ies", &e[..e.len()-1])); // library → libraries
            }
            if !e.ends_with("e") {
                covered.insert(format!("{}es", e));   // wish → wishes
            }
        }
    }

    // Per-token: how many distinct marketplace skills use it + a sample slug.
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    let mut sample: BTreeMap<String, String> = BTreeMap::new();

    let skills = idx.get("skills").and_then(|x| x.as_array());
    let skills = match skills {
        Some(s) => s,
        None => {
            println!("synonym-coverage: index has no `skills` array");
            return Ok(());
        }
    };

    let mut marketplace_count = 0_usize;
    for s in skills {
        if s.get("source").and_then(|x| x.as_str()) != Some("marketplace") { continue; }
        marketplace_count += 1;
        let slug = s.get("namespaced_slug").and_then(|x| x.as_str()).unwrap_or("");
        let tokens = match s.get("tokens").and_then(|x| x.as_array()) {
            Some(a) => a, None => continue,
        };
        // Dedupe per-skill so a token counted once per skill, not per occurrence.
        let mut per_skill: HashSet<String> = HashSet::new();
        for t in tokens {
            if let Some(tok) = t.as_str() {
                // Only ASCII tokens (skip Korean — those are alias artifacts,
                // not gaps in coverage).
                if !tok.chars().all(|c| c.is_ascii()) { continue; }
                // Skip the token IF it is already mapped from at least one Korean key.
                if covered.contains(tok) { continue; }
                // Skip if a singular variant is covered (e.g. tok="patterns" while
                // covered has "pattern"). Mirrors the Rust matcher's plural strip.
                let stripped: Option<String> = if tok.ends_with("ies") && tok.len() > 4 {
                    Some(format!("{}y", &tok[..tok.len()-3]))
                } else if tok.ends_with("es") && tok.len() > 3 {
                    Some(tok[..tok.len()-2].to_string())
                } else if tok.ends_with("s") && !tok.ends_with("ss") && tok.len() > 3 {
                    Some(tok[..tok.len()-1].to_string())
                } else if tok.ends_with("ing") && tok.len() > 5 {
                    Some(tok[..tok.len()-3].to_string())
                } else {
                    None
                };
                if let Some(s) = stripped {
                    if covered.contains(&s) { continue; }
                }
                // Skip very short / numeric-only edge cases (shouldn't occur given
                // _TOKEN_RE but defensive).
                if tok.len() < 3 { continue; }
                if tok.chars().all(|c| c.is_ascii_digit()) { continue; }
                if !per_skill.insert(tok.to_string()) { continue; }
                *freq.entry(tok.to_string()).or_insert(0) += 1;
                sample.entry(tok.to_string()).or_insert_with(|| slug.to_string());
            }
        }
    }

    if freq.is_empty() {
        println!("synonym-coverage: every English token in the index is already covered");
        println!("                  by KO_EN_SYNONYMS values (or no marketplace skills indexed)");
        return Ok(());
    }

    let mut ranked: Vec<(String, usize)> = freq.into_iter()
        .filter(|(_, n)| *n >= min_skills.max(1))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let total = ranked.len();
    println!("synonym-coverage: top {} uncovered English tokens (out of {} candidates, min-skills={})",
             ranked.len().min(top), total, min_skills.max(1));
    println!("                  scanned {} marketplace skills; KO_EN_SYNONYMS covers {} English values",
             marketplace_count, covered.len());
    println!();
    println!("  rank  token              skills  example slug");
    println!("  ----  -----------------  ------  -----------------------------------");
    for (i, (tok, n)) in ranked.iter().take(top).enumerate() {
        let sl = sample.get(tok).cloned().unwrap_or_default();
        let sl_short: String = sl.chars().take(45).collect();
        println!("  {:>4}  {:<17}  {:>5}   {}", i + 1, tok, n, sl_short);
    }
    println!();
    println!("These are English words appearing in {}+ skill descriptions with no Korean", min_skills.max(1));
    println!("alias mapped to them. A Korean user typing the Korean equivalent will miss");
    println!("every one of those skills. Consider adding mappings — edit BOTH:");
    println!("  learning/build_ecc_index.py  _KO_EN_SYNONYMS");
    println!("  rust/src/cli.rs              KO_EN_SYNONYMS");
    Ok(())
}
