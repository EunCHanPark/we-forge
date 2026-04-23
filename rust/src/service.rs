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

    /// getuid() — only meaningful on Unix. Returns 0 stub on non-Unix
    /// (Windows would never construct LaunchdManager anyway, but the code
    /// must compile for cross-platform builds).
    #[cfg(unix)]
    fn get_uid() -> u32 { unsafe { libc::getuid() } }
    #[cfg(not(unix))]
    fn get_uid() -> u32 { 0 }

    pub struct LaunchdManager {
        pub plist:   PathBuf,
        pub log_dir: PathBuf,
        pub uid:     u32,
    }

    impl LaunchdManager {
        pub fn new() -> Self {
            Self {
                plist:   paths::macos_launch_agents().join(format!("{}.plist", LABEL)),
                log_dir: paths::macos_log_dir(),
                uid:     get_uid(),
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
// Linux systemd (user-mode service + timer)
// ---------------------------------------------------------------------------

pub mod systemd {
    use super::*;
    use std::fs;

    pub const SERVICE_NAME: &str = "we-forge";

    pub struct SystemdManager {
        pub service_unit: PathBuf,
        pub timer_unit:   PathBuf,
        pub log_dir:      PathBuf,
    }

    impl SystemdManager {
        pub fn new() -> Self {
            let unit_dir = paths::linux_systemd_user_dir();
            Self {
                service_unit: unit_dir.join(format!("{}.service", SERVICE_NAME)),
                timer_unit:   unit_dir.join(format!("{}.timer", SERVICE_NAME)),
                log_dir:      paths::linux_state_dir(),
            }
        }

        fn generate_service(&self, daemon: bool) -> String {
            let exe = wforgectl_path();
            let cmd = if daemon { "daemon" } else { "run-once" };
            let restart = if daemon {
                "Restart=always\nRestartSec=30"
            } else {
                "Restart=on-failure\nRestartSec=30"
            };
            let claude_home = paths::claude_home().to_string_lossy().to_string();
            let we_forge_home = paths::we_forge_home().to_string_lossy().to_string();
            let log = self.log_dir.to_string_lossy().to_string();
            let mode_label = if daemon { "daemon" } else { "scheduled" };

            format!(
"[Unit]
Description=we-forge pattern-learning service ({mode_label})
After=network.target

[Service]
Type=simple
ExecStart={exe} {cmd}
{restart}
StandardOutput=append:{log}/daemon.log
StandardError=append:{log}/daemon.log
Environment=CLAUDE_HOME={claude_home}
Environment=WE_FORGE_HOME={we_forge_home}

[Install]
WantedBy=default.target
"
            )
        }

        fn generate_timer(&self) -> String {
            format!(
"[Unit]
Description=we-forge hourly tick timer
Requires={SERVICE_NAME}.service

[Timer]
OnBootSec=5min
OnUnitActiveSec=1h
Persistent=true
Unit={SERVICE_NAME}.service

[Install]
WantedBy=timers.target
"
            )
        }

        fn systemctl_user(args: &[&str]) -> std::io::Result<std::process::Output> {
            let mut full = vec!["--user"];
            full.extend(args);
            Command::new("systemctl").args(&full).output()
        }
    }

    impl ServiceManager for SystemdManager {
        fn install(&self, daemon: bool) -> Result<()> {
            // Pre-flight: systemctl must exist
            if Command::new("systemctl").arg("--version").output().is_err() {
                return Err(anyhow!("systemctl not found — this tool requires systemd"));
            }

            fs::create_dir_all(&self.log_dir).context("create log dir")?;
            fs::create_dir_all(self.service_unit.parent().unwrap()).context("create unit dir")?;

            // Write service unit
            atomic_write(&self.service_unit, self.generate_service(daemon).as_bytes(), 0o600)?;

            // Timer only in scheduled mode (daemon mode runs continuously)
            if !daemon {
                atomic_write(&self.timer_unit, self.generate_timer().as_bytes(), 0o600)?;
            } else if self.timer_unit.exists() {
                let _ = fs::remove_file(&self.timer_unit);
            }

            // Reload + enable + start
            let r = Self::systemctl_user(&["daemon-reload"])?;
            if !r.status.success() {
                return Err(anyhow!("daemon-reload failed: {}", String::from_utf8_lossy(&r.stderr)));
            }

            let target = if daemon {
                format!("{}.service", SERVICE_NAME)
            } else {
                format!("{}.timer", SERVICE_NAME)
            };
            let r = Self::systemctl_user(&["enable", "--now", &target])?;
            if !r.status.success() {
                return Err(anyhow!(
                    "systemctl enable --now {} failed: {}",
                    target, String::from_utf8_lossy(&r.stderr)
                ));
            }

            // loginctl enable-linger reminder (so service persists after logout)
            if let Ok(user) = std::env::var("USER") {
                let r = Command::new("loginctl")
                    .args(["show-user", &user, "-p", "Linger"])
                    .output();
                if let Ok(o) = r {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    if !stdout.contains("Linger=yes") {
                        eprintln!(
                            "  WARN to keep service running after logout, run: sudo loginctl enable-linger {}",
                            user
                        );
                    }
                }
            }
            Ok(())
        }

        fn uninstall(&self) -> Result<()> {
            for unit in [
                format!("{}.timer", SERVICE_NAME),
                format!("{}.service", SERVICE_NAME),
            ] {
                let _ = Self::systemctl_user(&["disable", "--now", &unit]);
            }
            for u in [&self.timer_unit, &self.service_unit] {
                if u.exists() {
                    fs::remove_file(u).with_context(|| format!("remove {}", u.display()))?;
                }
            }
            let _ = Self::systemctl_user(&["daemon-reload"]);
            Ok(())
        }

        fn start(&self) -> Result<()> {
            let target = if self.timer_unit.exists() {
                format!("{}.timer", SERVICE_NAME)
            } else {
                format!("{}.service", SERVICE_NAME)
            };
            let r = Self::systemctl_user(&["start", &target])?;
            if !r.status.success() {
                return Err(anyhow!("start failed: {}", String::from_utf8_lossy(&r.stderr)));
            }
            Ok(())
        }

        fn stop(&self) -> Result<()> {
            for unit in [
                format!("{}.timer", SERVICE_NAME),
                format!("{}.service", SERVICE_NAME),
            ] {
                let _ = Self::systemctl_user(&["stop", &unit]);
            }
            Ok(())
        }

        fn status(&self) -> Status {
            if !self.service_unit.exists() && !self.timer_unit.exists() {
                return Status::NotInstalled;
            }
            let target = if self.timer_unit.exists() {
                format!("{}.timer", SERVICE_NAME)
            } else {
                format!("{}.service", SERVICE_NAME)
            };
            match Self::systemctl_user(&["is-active", &target]) {
                Ok(o) => {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    match s.as_str() {
                        "active"   => Status::Running,
                        _          => Status::Stopped,
                    }
                }
                Err(_) => Status::Stopped,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Windows Task Scheduler (PowerShell shellouts)
//
// Implemented via PowerShell because:
//   - Stable, ships with every Windows install
//   - Same scripts as install.ps1 (proven path)
//   - COM-based native impl is a v0.4 follow-up
// ---------------------------------------------------------------------------

pub mod taskscheduler {
    use super::*;

    pub const TASK_NAME: &str = "we-forge";

    pub struct TaskSchedulerManager;

    impl TaskSchedulerManager {
        pub fn new() -> Self { Self }

        /// Run a PowerShell script, return its Output.
        fn powershell(script: &str) -> Result<std::process::Output> {
            let r = Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .output()
                .context("powershell invocation failed")?;
            Ok(r)
        }

        fn install_script(daemon: bool) -> String {
            let exe = wforgectl_path();
            let cmd = if daemon { "daemon" } else { "run-once" };
            // PowerShell single-quote escaping: ' becomes ''
            let exe_esc = exe.replace('\'', "''");
            let trigger = if daemon {
                "$trigger = New-ScheduledTaskTrigger -AtLogOn".to_string()
            } else {
                "$trigger = New-ScheduledTaskTrigger -Once \
                 -At ([DateTime]::Now.Date.AddHours([DateTime]::Now.Hour + 1)) \
                 -RepetitionInterval (New-TimeSpan -Hours 1)".to_string()
            };
            format!(
"$action = New-ScheduledTaskAction -Execute '{exe_esc}' -Argument '{cmd}'
{trigger}
$settings = New-ScheduledTaskSettingsSet \
  -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries \
  -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Minutes 10)
$principal = New-ScheduledTaskPrincipal -UserId \"$env:USERNAME\" -LogonType Interactive
$task = New-ScheduledTask -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Description 'we-forge pattern-learning service'
Unregister-ScheduledTask -TaskName '{name}' -Confirm:$false -ErrorAction SilentlyContinue
Register-ScheduledTask -TaskName '{name}' -InputObject $task | Out-Null
Write-Output 'registered'",
                name = TASK_NAME,
            )
        }
    }

    impl ServiceManager for TaskSchedulerManager {
        fn install(&self, daemon: bool) -> Result<()> {
            let script = Self::install_script(daemon);
            let r = Self::powershell(&script)?;
            if !r.status.success() {
                return Err(anyhow!(
                    "Register-ScheduledTask failed: {}",
                    String::from_utf8_lossy(&r.stderr).trim()
                ));
            }
            Ok(())
        }

        fn uninstall(&self) -> Result<()> {
            let script = format!(
                "Unregister-ScheduledTask -TaskName '{}' -Confirm:$false -ErrorAction SilentlyContinue; Write-Output 'ok'",
                TASK_NAME
            );
            let _ = Self::powershell(&script)?;
            Ok(())
        }

        fn start(&self) -> Result<()> {
            let script = format!("Start-ScheduledTask -TaskName '{}'", TASK_NAME);
            let r = Self::powershell(&script)?;
            if !r.status.success() {
                return Err(anyhow!(
                    "Start-ScheduledTask failed: {}",
                    String::from_utf8_lossy(&r.stderr).trim()
                ));
            }
            Ok(())
        }

        fn stop(&self) -> Result<()> {
            let script = format!("Stop-ScheduledTask -TaskName '{}'", TASK_NAME);
            let _ = Self::powershell(&script)?;
            Ok(())
        }

        fn status(&self) -> Status {
            let script = format!(
                "$t = Get-ScheduledTask -TaskName '{}' -ErrorAction SilentlyContinue
                 if (-not $t) {{ Write-Output '__MISSING__' }} else {{ Write-Output $t.State }}",
                TASK_NAME
            );
            let r = match Self::powershell(&script) {
                Ok(o) => o,
                Err(_) => return Status::NotInstalled,
            };
            let stdout = String::from_utf8_lossy(&r.stdout);
            let s = stdout
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("__MISSING__");
            match s {
                "__MISSING__" => Status::NotInstalled,
                "Running"     => Status::Running,
                _             => Status::Stopped,
            }
        }
    }
}
