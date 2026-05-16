use super::*;

pub fn log(skill: &str, reason: &str, invoker: &str) -> Result<()> {
    if skill.is_empty() {
        return Err(anyhow::anyhow!("skill name required"));
    }
    ecc_core::log(skill, reason, invoker)?;
    println!("  OK logged: {skill}");
    Ok(())
}

pub fn trace(last_n: usize, group: bool) -> Result<()> {
    let entries = ecc_core::read_all();
    if entries.is_empty() {
        println!("  WARN no ECC trace yet: {}", paths::ecc_trace_file().display());
        println!("  the we-forge agent (or CLI) calls 'we-forgectl ecc-log <skill> <reason>'");
        return Ok(());
    }
    if group {
        let mut counter: BTreeMap<String, usize> = BTreeMap::new();
        for e in &entries {
            *counter.entry(e.skill.clone()).or_insert(0) += 1;
        }
        let mut sorted: Vec<_> = counter.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        println!("==> ECC skill usage (totals across {} records)", entries.len());
        for (skill, n) in sorted {
            println!("  {n:>4}  {skill}");
        }
        return Ok(());
    }
    println!("==> ECC trace (last {last_n} of {})", entries.len());
    let start = entries.len().saturating_sub(last_n);
    for e in &entries[start..] {
        println!("  {}  [{}]  {}  {}",
            e.ts,
            e.invoker,
            e.skill,
            e.reason.chars().take(80).collect::<String>(),
        );
    }
    Ok(())
}
