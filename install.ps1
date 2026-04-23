# install.ps1 — we-forge Windows installer
#
# Usage (one-line, from PowerShell):
#   iwr -useb https://raw.githubusercontent.com/EunCHanPark/we-forge/main/install.ps1 | iex
#
# Or after cloning the repo:
#   .\install.ps1
#   .\install.ps1 -DryRun
#   .\install.ps1 -Branch main
#   .\install.ps1 -SkipScheduler
#
# What this script does:
#   1. Verifies WSL2 is installed (offers to install if missing — needs admin).
#   2. Inside the default WSL2 distro, clones we-forge into ~/we-forge (or
#      pulls if already there).
#   3. Runs the bash install.sh inside WSL2.
#   4. Registers a Windows Task Scheduler job that fires hourly and runs
#      `wsl.exe -- bash ~/.claude/learning/tick.sh`. This way the learning
#      loop ticks even when no WSL2 terminal is open.
#
# What this script does NOT do:
#   - Modify your Windows PATH or registry beyond Task Scheduler entry.
#   - Install Claude Code itself — install Claude Code first inside WSL2.
#   - Run anything as administrator unless WSL2 install is needed.

[CmdletBinding()]
param(
    [string]$Branch = "main",
    [string]$Repo   = "https://github.com/EunCHanPark/we-forge.git",
    [switch]$DryRun,
    [switch]$SkipScheduler,
    [switch]$SkipWslCheck
)

$ErrorActionPreference = "Stop"
$ProgressPreference    = "SilentlyContinue"
$script:TaskName       = "we-forge-tick"

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "    OK   $msg" -ForegroundColor Green }
function Write-Warn2($msg){ Write-Host "    WARN $msg" -ForegroundColor Yellow }
function Write-Err2($msg) { Write-Host "    FAIL $msg" -ForegroundColor Red }

# ---------------------------------------------------------------------------
# 1. WSL2 check
# ---------------------------------------------------------------------------

function Test-Wsl2 {
    if ($SkipWslCheck) {
        Write-Warn2 "skipping WSL check (--SkipWslCheck)"
        return $true
    }
    Write-Step "checking WSL2"

    $wslPath = (Get-Command wsl.exe -ErrorAction SilentlyContinue)
    if (-not $wslPath) {
        Write-Err2 "wsl.exe not found"
        Write-Host @"

    we-forge requires WSL2 on Windows. Install with (admin PowerShell):
        wsl --install

    Then sign out / reboot, set up a Linux user, and re-run this installer.
    See WSL-SETUP.md for the full guide.
"@
        return $false
    }

    # Probe default distro
    $listOutput = & wsl.exe -l -v 2>&1 | Out-String
    if ($listOutput -match "Windows Subsystem for Linux has no installed distributions" -or
        $listOutput -match "no installed distributions") {
        Write-Err2 "WSL2 has no installed distribution"
        Write-Host @"

    Install one (e.g. Ubuntu) with:
        wsl --install -d Ubuntu

    Then re-run this installer.
"@
        return $false
    }

    # Verify it's version 2 (not legacy v1)
    if ($listOutput -notmatch '\s+2\s*$' -and $listOutput -notmatch '\s+2\s+') {
        Write-Warn2 "default distro may be WSL v1 — recommend upgrading: wsl --set-default-version 2"
    }

    Write-Ok "WSL detected"
    return $true
}

# ---------------------------------------------------------------------------
# 2. Clone + install inside WSL2
# ---------------------------------------------------------------------------

function Invoke-WslInstall {
    Write-Step "cloning + installing inside WSL2"

    # Build a single bash command that:
    #   - checks for git/jq/python3
    #   - clones or pulls the repo
    #   - runs install.sh
    $installFlag = if ($DryRun) { "--dry-run" } else { "" }
    $bashCmd = @"
set -e
for tool in git jq python3 bash; do
  command -v `$tool >/dev/null || { echo "missing: `$tool — sudo apt install -y `$tool"; exit 1; }
done

REPO_DIR="`$HOME/we-forge"
if [ -d "`$REPO_DIR/.git" ]; then
  cd "`$REPO_DIR"
  echo "  pulling latest..."
  git fetch --depth 1 origin "$Branch"
  git checkout "$Branch"
  git reset --hard "origin/$Branch"
else
  echo "  cloning $Repo (branch $Branch) into `$REPO_DIR..."
  git clone --depth 1 --branch "$Branch" "$Repo" "`$REPO_DIR"
  cd "`$REPO_DIR"
fi

chmod +x install.sh verify.sh learning/*.sh hooks/*.sh
./install.sh $installFlag
"@

    if ($DryRun) {
        Write-Host "    DRY  would run inside WSL:" -ForegroundColor DarkGray
        $bashCmd -split "`n" | ForEach-Object { Write-Host "         $_" -ForegroundColor DarkGray }
    } else {
        # Pipe via stdin to avoid quoting headaches
        $bashCmd | & wsl.exe bash -s
        if ($LASTEXITCODE -ne 0) {
            Write-Err2 "WSL install.sh failed (exit $LASTEXITCODE)"
            exit 1
        }
        Write-Ok "WSL install.sh completed"
    }
}

# ---------------------------------------------------------------------------
# 3. Windows Task Scheduler
# ---------------------------------------------------------------------------

function Install-TaskScheduler {
    if ($SkipScheduler) {
        Write-Warn2 "skipping Task Scheduler (--SkipScheduler)"
        return
    }
    Write-Step "registering Windows Task Scheduler job '$script:TaskName'"

    # Trigger: hourly starting at next :00, indefinitely
    $now        = Get-Date
    $startTime  = $now.Date.AddHours($now.Hour + 1)
    $trigger    = New-ScheduledTaskTrigger -Once -At $startTime `
                    -RepetitionInterval (New-TimeSpan -Hours 1)

    # Action: invoke wsl.exe with we-forgectl (the unified entry point).
    # we-forgectl run-once is equivalent to tick.sh but routes through the
    # service manager so logs land in ~/Library/Logs/we-forge/daemon.log
    # consistently with the other platforms.
    # Note: we-forgectl install (called inside WSL by install.sh) ALSO creates
    # a systemd timer, which is harmless overlap — tick.sh has its own mkdir
    # lock so double-fires are no-ops, and the Windows Task Scheduler is what
    # actually wakes WSL2 when no terminal is open.
    $action = New-ScheduledTaskAction `
        -Execute "wsl.exe" `
        -Argument "-- bash -lc `"~/.local/bin/we-forgectl run-once 2>/dev/null || ~/.claude/learning/tick.sh`""

    # Settings: allow on-battery, restart on failure, bounded runtime
    $settings = New-ScheduledTaskSettingsSet `
        -AllowStartIfOnBatteries `
        -DontStopIfGoingOnBatteries `
        -StartWhenAvailable `
        -ExecutionTimeLimit (New-TimeSpan -Minutes 10)

    $principal = New-ScheduledTaskPrincipal -UserId "$env:USERNAME" -LogonType Interactive

    $task = New-ScheduledTask `
        -Action $action -Trigger $trigger -Settings $settings -Principal $principal `
        -Description "we-forge hourly pattern-learning tick (runs inside WSL2)"

    if ($DryRun) {
        Write-Host "    DRY  would register task '$script:TaskName' starting $startTime, hourly" -ForegroundColor DarkGray
    } else {
        # Replace if already exists
        Unregister-ScheduledTask -TaskName $script:TaskName -Confirm:$false -ErrorAction SilentlyContinue
        Register-ScheduledTask -TaskName $script:TaskName -InputObject $task | Out-Null
        Write-Ok "task registered (next run: $startTime, then hourly)"
    }
}

# ---------------------------------------------------------------------------
# 4. Verify
# ---------------------------------------------------------------------------

function Invoke-Verify {
    if ($DryRun) {
        Write-Host "    DRY  would run verify.sh inside WSL" -ForegroundColor DarkGray
        return
    }
    Write-Step "running verify.sh inside WSL"
    & wsl.exe bash -lc "cd ~/we-forge && ./verify.sh"
    if ($LASTEXITCODE -ne 0) {
        Write-Warn2 "verify.sh reported issues (exit $LASTEXITCODE) — see output above"
    } else {
        Write-Ok "verify.sh: all checks passed"
    }
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

Write-Host ""
Write-Host "we-forge - Windows installer" -ForegroundColor White
Write-Host "  branch: $Branch" -ForegroundColor DarkGray
Write-Host "  repo:   $Repo"   -ForegroundColor DarkGray
if ($DryRun) { Write-Host "  mode:   DRY RUN (no changes will be made)" -ForegroundColor Yellow }
Write-Host ""

if (-not (Test-Wsl2)) { exit 1 }
Invoke-WslInstall
Install-TaskScheduler
Invoke-Verify

Write-Host ""
Write-Host "Done. Useful next steps:" -ForegroundColor Green
Write-Host "  - Inside WSL2:        tail -f ~/.claude/learning/data/tick.log"
Write-Host "  - Inside Claude Code: /skill-report"
Write-Host "  - Check task:         Get-ScheduledTask -TaskName '$script:TaskName'"
Write-Host ""
