use super::*;
use std::fs;

pub fn run(deep: bool) -> Result<()> {
    println!("==> we-forgectl uninstall (safety-guard pattern)");

    fs::create_dir_all(paths::backup_dir())?;
    let snap = paths::backup_dir().join(now_iso());
    fs::create_dir_all(&snap)?;
    if paths::config_file().exists() {
        let _ = fs::copy(paths::config_file(), snap.join("config.json"));
    }
    let claude_settings = paths::claude_home().join("settings.json");
    if claude_settings.exists() {
        let _ = fs::copy(&claude_settings, snap.join("settings.json"));
    }
    println!("  OK backup: {}", snap.display());

    let m = service::manager();
    let _ = m.stop();
    m.uninstall()?;
    println!("  OK service uninstalled");

    if deep {
        let we_forge_home = paths::we_forge_home();
        if we_forge_home.exists() {
            let dest = snap.join("we-forge-home");
            let _ = fs::rename(&we_forge_home, &dest);
            println!("  OK ~/.we-forge moved to {}", dest.display());
        }
        let learning_data = paths::claude_home().join("learning/data");
        if learning_data.exists() {
            let dest = snap.join("learning-data");
            let _ = fs::rename(&learning_data, &dest);
            println!("  OK ~/.claude/learning/data moved to {}", dest.display());
        }
    }

    let _ = ecc_core::log("safety-guard", "uninstall via we-forgectl (backup created)", "cli");
    println!("\n==> uninstalled. backup at: {}", snap.display());
    Ok(())
}
