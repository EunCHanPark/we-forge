use super::*;
use std::fs;

pub fn run(n: usize) -> Result<()> {
    let candidates = [
        paths::macos_log_dir().join("daemon.log"),
        paths::linux_state_dir().join("daemon.log"),
        paths::claude_home().join("learning/data/tick.log"),
    ];
    for path in candidates {
        if path.exists() {
            println!("==> tail -n {n} {}", path.display());
            let text = fs::read_to_string(&path)?;
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(n);
            for line in &lines[start..] {
                println!("  {line}");
            }
            return Ok(());
        }
    }
    eprintln!("  WARN no log files found yet");
    Ok(())
}
