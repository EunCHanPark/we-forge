use super::*;
use std::path::Path;

fn which(tool: &str) -> Option<std::path::PathBuf> {
    // On Windows, binaries live as tool.exe / tool.cmd / tool.bat on PATH.
    // Searching for the bare name misses them and produces false FAIL
    // reports in `doctor` even when the tool is installed.
    let exts: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::var("PATH").ok().and_then(|paths| {
        paths
            .split(if cfg!(windows) { ';' } else { ':' })
            .flat_map(|d| {
                exts.iter().map(move |e| {
                    if e.is_empty() {
                        Path::new(d).join(tool)
                    } else {
                        Path::new(d).join(format!("{tool}{e}"))
                    }
                })
            })
            .find(|p| p.is_file())
    })
}

pub fn run() -> Result<()> {
    println!("==> doctor");
    let mut issues = 0;
    for tool in ["python3", "bash", "jq"] {
        if let Some(p) = which(tool) {
            println!("  OK {tool}: {}", p.display());
        } else {
            println!("  FAIL {tool} not in PATH");
            issues += 1;
        }
    }
    for (path, label) in [
        (paths::claude_home().join("learning/tick.sh"), "tick.sh"),
        (paths::claude_home().join("agents/we-forge.md"), "we-forge agent"),
        (paths::claude_home().join("settings.json"), "settings.json"),
    ] {
        if path.exists() {
            println!("  OK {label}: {}", path.display());
        } else {
            println!("  FAIL {label} missing: {}", path.display());
            issues += 1;
        }
    }
    let s = service::manager().status();
    println!("  service: {s}");
    if issues == 0 {
        println!("\nall checks passed.");
        Ok(())
    } else {
        eprintln!("\n{issues} issue(s)");
        Err(anyhow::anyhow!("{issues} doctor issue(s)"))
    }
}
