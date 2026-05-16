use super::*;
use std::process::Command;

pub fn run() -> Result<()> {
    let candidates = [
        std::env::current_exe()?.parent().map(|p| p.join("../dashboard/dashboard.py")),
        Some(dirs::home_dir().unwrap().join("we-forge/dashboard/dashboard.py")),
    ];
    for path in candidates.iter().flatten() {
        if path.exists() {
            let _ = ecc_core::log("dashboard-builder", "dashboard.py launched via we-forgectl", "cli");
            let status = Command::new("python3").arg(path).arg("--serve").status()?;
            std::process::exit(status.code().unwrap_or(0));
        }
    }
    eprintln!("  FAIL dashboard.py not found");
    Err(anyhow::anyhow!("dashboard.py not found"))
}
