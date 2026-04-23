//! Service-manager abstraction across launchd / systemd / Task Scheduler.
//!
//! ECC alignment:
//!   - service::*               → enterprise-agent-ops (lifecycle management)
//!   - service::launchd::install → autonomous-agent-harness (KeepAlive=true daemon)
//!   - service::*::migrate       → safety-guard (backup-before-replace)

use crate::core::{atomic_write, now_iso, paths, Os};
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Running,
    Stopped,
    NotInstalled,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Running       => write!(f, "running"),
            Status::Stopped       => write!(f, "stopped"),
            Status::NotInstalled  => write!(f, "not-installed"),
        }
    }
}

pub trait ServiceManager: Send + Sync {
    fn install(&self, daemon: bool) -> Result<()>;
    fn uninstall(&self) -> Result<()>;
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn restart(&self) -> Result<()> {
        let _ = self.stop();
        std::thread::sleep(std::time::Duration::from_millis(500));
        self.start()
    }
    fn status(&self) -> Status;
    fn migrate_legacy(&self) -> Result<()> { Ok(()) }
}

pub fn manager() -> Box<dyn ServiceManager> {
    match Os::detect() {
        Os::MacOS   => Box::new(launchd::LaunchdManager::new()),
        Os::Linux   => Box::new(systemd::SystemdManager::new()),
        Os::Windows => Box::new(taskscheduler::TaskSchedulerManager::new()),
        Os::Other   => panic!("unsupported OS"),
    }
}

fn wforgectl_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "we-forgectl".to_string())
}

// ---------------------------------------------------------------------------
// macOS launchd
// ---------------------------------------------------------------------------

pub mod launchd {
    use super::*;
    use std::fs;

    pub const LABEL: &str = "com.we-forge.daemon";
    pub const LEGACY_LABELS: &[&str] = &["com.yukibana.we-forge-tick"];

    pub struct LaunchdManager {
        pub plist:   PathBuf,
        pub log_dir: PathBuf,
        pub uid:     u32,
    }

    impl LaunchdManager {
        pub fn new() -> Self {
            let uid = unsafe { libc::getuid() };
            Self {
                plist:   paths::macos_launch_agents().join(format!("{}.plist", LABEL)),
                log_dir: paths::macos_log_dir(),
                uid,
            }
        }

        fn domain(&self) -> String { format!("gui/{}", self.uid) }
        fn target(&self) -> String { format!("{}/{}", self.domain(), LABEL) }

        fn generate_plist(&self, daemon: bool) -> String {
            let exe = wforgectl_path();
            let log = self.log_dir.join("daemon.log").to_string_lossy().to_string();
            let claude_home = paths::claude_home().to_string_lossy().to_string();
            let we_forge_home = paths::we_forge_home().to_string_lossy().to_string();

            let (program_args, schedule_block) = if daemon {
                (
                    format!("        <string>{}</string>\n        <string>daemon</string>", exe),
                    "    <key>RunAtLoad</key><true/>\n    <key>KeepAlive</key><true/>".to_string(),
                )
            } else {
                (
                    format!("        <string>{}</string>\n        <string>run-once</string>", exe),
                    "    <key>StartCalendarInterval</key>\n    <dict><key>Minute</key><integer>0</integer></dict>\n    <key>RunAtLoad</key><false/>".to_string(),
                )
            };

            format!(
r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
{program_args}
    </array>
{schedule_block}
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
        <key>CLAUDE_HOME</key>
        <string>{claude_home}</string>
        <key>WE_FORGE_HOME</key>
        <string>{we_forge_home}</string>
    </dict>
</dict>
</plist>
"#,
                label = LABEL,
                program_args = program_args,
                schedule_block = schedule_block,
                log = log,
                claude_home = claude_home,
                we_forge_home = we_forge_home,
            )
        }
    }

    impl ServiceManager for LaunchdManager {
        fn install(&self, daemon: bool) -> Result<()> {
            fs::create_dir_all(&self.log_dir).context("create log dir")?;
            fs::create_dir_all(self.plist.parent().unwrap()).context("create LaunchAgents dir")?;
            let body = self.generate_plist(daemon);
            atomic_write(&self.plist, body.as_bytes(), 0o600)?;
            let _ = Command::new("launchctl").args(["enable", &self.target()]).output();
            let r = Command::new("launchctl")
                .args(["bootstrap", &self.domain(), self.plist.to_string_lossy().as_ref()])
                .output()?;
            if !r.status.success() {
                let stderr = String::from_utf8_lossy(&r.stderr);
                if !stderr.to_lowercase().contains("already loaded") {
                    let _ = Command::new("launchctl")
                        .args(["kickstart", "-k", &self.target()])
                        .output();
                }
            }
            Ok(())
        }

        fn uninstall(&self) -> Result<()> {
            if self.plist.exists() {
                let _ = Command::new("launchctl").args(["bootout", &self.target()]).output();
                fs::remove_file(&self.plist).context("remove plist")?;
            }
            Ok(())
        }

        fn start(&self) -> Result<()> {
            let r = Command::new("launchctl").args(["kickstart", &self.target()]).output()?;
            if !r.status.success() {
                return Err(anyhow!("kickstart failed: {}", String::from_utf8_lossy(&r.stderr)));
            }
            Ok(())
        }

        fn stop(&self) -> Result<()> {
            let _ = Command::new("launchctl").args(["stop", LABEL]).output();
            Ok(())
        }

        fn status(&self) -> Status {
            if !self.plist.exists() {
                return Status::NotInstalled;
            }
            let r = match Command::new("launchctl").args(["print", &self.target()]).output() {
                Ok(o) if o.status.success() => o,
                _ => return Status::Stopped,
            };
            let text = String::from_utf8_lossy(&r.stdout);
            let mut has_pid = false;
            let mut state_running = false;
            for line in text.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("pid = ") {
                    if rest.trim().parse::<u32>().is_ok() {
                        has_pid = true;
                    }
                }
                if t == "state = running" {
                    state_running = true;
                }
            }
            if has_pid || state_running { Status::Running } else { Status::Stopped }
        }

        fn migrate_legacy(&self) -> Result<()> {
            for legacy in LEGACY_LABELS {
                let legacy_plist = paths::macos_launch_agents().join(format!("{}.plist", legacy));
                if !legacy_plist.exists() { continue; }
                let _ = Command::new("launchctl")
                    .args(["bootout", &format!("{}/{}", self.domain(), legacy)])
                    .output();
                fs::create_dir_all(paths::backup_dir())?;
                let backup = paths::backup_dir()
                    .join(format!("{}.{}.plist", legacy, now_iso()));
                fs::copy(&legacy_plist, &backup).context("backup legacy plist")?;
                fs::remove_file(&legacy_plist).context("remove legacy plist")?;
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Linux systemd (stub — port from Python in next session)
// ---------------------------------------------------------------------------

pub mod systemd {
    use super::*;

    pub struct SystemdManager;

    impl SystemdManager {
        pub fn new() -> Self { Self }
    }

    impl ServiceManager for SystemdManager {
        fn install(&self, _daemon: bool) -> Result<()> {
            Err(anyhow!("systemd manager: TODO — see scripts/we-forgectl (Python) for reference impl"))
        }
        fn uninstall(&self) -> Result<()> { Err(anyhow!("systemd: TODO")) }
        fn start(&self)     -> Result<()> { Err(anyhow!("systemd: TODO")) }
        fn stop(&self)      -> Result<()> { Err(anyhow!("systemd: TODO")) }
        fn status(&self)    -> Status     { Status::NotInstalled }
    }
}

// ---------------------------------------------------------------------------
// Windows Task Scheduler (stub — port from Python in next session)
// ---------------------------------------------------------------------------

pub mod taskscheduler {
    use super::*;

    pub struct TaskSchedulerManager;

    impl TaskSchedulerManager {
        pub fn new() -> Self { Self }
    }

    impl ServiceManager for TaskSchedulerManager {
        fn install(&self, _daemon: bool) -> Result<()> {
            Err(anyhow!("Task Scheduler: TODO — see scripts/we-forgectl (Python) for reference impl"))
        }
        fn uninstall(&self) -> Result<()> { Err(anyhow!("Task Scheduler: TODO")) }
        fn start(&self)     -> Result<()> { Err(anyhow!("Task Scheduler: TODO")) }
        fn stop(&self)      -> Result<()> { Err(anyhow!("Task Scheduler: TODO")) }
        fn status(&self)    -> Status     { Status::NotInstalled }
    }
}
