use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(label: &str) -> Result<()> {
    let dir = paths::heartbeats_dir();
    fs::create_dir_all(&dir)?;
    let cwd = std::env::current_dir()?;
    let cwd_str = cwd.to_string_lossy().to_string();
    let pid = std::process::id();
    let epoch = SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let record = serde_json::json!({
        "ts":    now_iso(),
        "epoch": epoch,
        "cwd":   cwd_str,
        "pid":   pid,
        "label": label,
    });
    let path = dir.join(format!("{pid}.json"));
    fs::write(&path, record.to_string())?;
    println!("================================================================");
    println!("✅  we-forge attached to this session");
    println!("    cwd:   {cwd_str}");
    println!("    pid:   {pid}");
    if !label.is_empty() {
        println!("    label: {label}");
    }
    println!("================================================================");
    println!("  we-forgectl sessions  — to verify this session is visible");
    Ok(())
}
