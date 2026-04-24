# install.ps1 — we-forge native Windows installer
#
# Downloads the prebuilt we-forgectl.exe from the latest GitHub Release,
# extracts it to ~/.local/bin, registers user PATH, and runs the native
# `we-forgectl install` (which sets up Windows Task Scheduler).
#
# One-line install (no clone, no WSL required):
#
#   iwr -useb https://raw.githubusercontent.com/EunCHanPark/we-forge/main/install.ps1 | iex
#
# Or with options after cloning:
#
#   .\install.ps1                                 # default — latest release
#   .\install.ps1 -Version v0.4.0                 # pin to specific version
#   .\install.ps1 -InstallDir "C:\tools\we-forge" # custom install location
#   .\install.ps1 -NoPathSetup                    # skip PATH registration
#   .\install.ps1 -NoServiceInstall               # skip Task Scheduler setup
#   .\install.ps1 -EnableTelegram                 # enable Telegram daemon mode
#
# WSL2 fallback (legacy, still supported):
#
#   See WSL-SETUP.md for the WSL2-based install path. The previous
#   `install.ps1` (which clones into WSL2 and uses bash install.sh) is
#   preserved as install.ps1.wsl-fallback.bak in repo clones.

[CmdletBinding()]
param(
    [string]$InstallDir       = "$env:USERPROFILE\.local\bin",
    [string]$Version          = "latest",
    [switch]$NoPathSetup,
    [switch]$NoServiceInstall,
    [switch]$EnableTelegram
)

$ErrorActionPreference = "Stop"
$ProgressPreference    = "SilentlyContinue"

function Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function OK($msg)   { Write-Host "    OK   $msg" -ForegroundColor Green }
function Warn($msg) { Write-Host "    WARN $msg" -ForegroundColor Yellow }
function Fail($msg) { Write-Host "    FAIL $msg" -ForegroundColor Red; exit 1 }

# ---------------------------------------------------------------------------
# 0. Banner
# ---------------------------------------------------------------------------

Write-Host ""
Write-Host "we-forge - Windows native installer" -ForegroundColor White
Write-Host "  install dir:    $InstallDir" -ForegroundColor DarkGray
Write-Host "  version:        $Version" -ForegroundColor DarkGray
Write-Host ""

# ---------------------------------------------------------------------------
# 1. Determine download URL + temp paths
# ---------------------------------------------------------------------------

$ZipName = "we-forgectl-x86_64-pc-windows-msvc.zip"
$ReleaseUrl = if ($Version -eq "latest") {
    "https://github.com/EunCHanPark/we-forge/releases/latest/download/$ZipName"
} else {
    "https://github.com/EunCHanPark/we-forge/releases/download/$Version/$ZipName"
}
$TempZip = Join-Path $env:TEMP "we-forgectl-install-$([System.IO.Path]::GetRandomFileName()).zip"

# ---------------------------------------------------------------------------
# 2. Download
# ---------------------------------------------------------------------------

Step "downloading we-forgectl ($Version)"
Write-Host "    from: $ReleaseUrl" -ForegroundColor DarkGray
try {
    Invoke-WebRequest -Uri $ReleaseUrl -OutFile $TempZip -UseBasicParsing
    OK "downloaded $([math]::Round((Get-Item $TempZip).Length / 1MB, 2)) MB"
} catch {
    Fail "download failed: $($_.Exception.Message)"
}

# ---------------------------------------------------------------------------
# 3. Extract
# ---------------------------------------------------------------------------

Step "extracting to $InstallDir"
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}
try {
    Expand-Archive -Path $TempZip -DestinationPath $InstallDir -Force
    Remove-Item $TempZip -Force -ErrorAction SilentlyContinue
} catch {
    Fail "extract failed: $($_.Exception.Message)"
}

$ExePath = Join-Path $InstallDir "we-forgectl.exe"
if (-not (Test-Path $ExePath)) {
    Fail "we-forgectl.exe not found at $ExePath after extract"
}
OK "we-forgectl.exe -> $ExePath"

# ---------------------------------------------------------------------------
# 4. Register PATH (user scope, persists)
# ---------------------------------------------------------------------------

if (-not $NoPathSetup) {
    Step "registering PATH (user scope)"
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $userPath) { $userPath = "" }

    $pathParts = $userPath -split ";" | Where-Object { $_ -ne "" }
    if ($pathParts -contains $InstallDir) {
        OK "$InstallDir already in user PATH"
    } else {
        $newPath = if ($userPath -eq "") { $InstallDir } else { "$userPath;$InstallDir" }
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        OK "added $InstallDir to user PATH (persistent)"
    }
    # Also reflect in the current session immediately
    if ($env:Path -notlike "*$InstallDir*") {
        $env:Path = "$env:Path;$InstallDir"
        OK "PATH updated in current session"
    }
} else {
    Warn "skipping PATH setup (-NoPathSetup)"
    Warn "you must invoke we-forgectl by full path: $ExePath"
}

# ---------------------------------------------------------------------------
# 5. Smoke test
# ---------------------------------------------------------------------------

Step "smoke test: we-forgectl --version"
try {
    $version = & $ExePath --version 2>&1
    OK "binary works -> $version"
} catch {
    Fail "smoke test failed: $($_.Exception.Message)"
}

# ---------------------------------------------------------------------------
# 6. Service install (Windows Task Scheduler via Rust manager)
# ---------------------------------------------------------------------------

if (-not $NoServiceInstall) {
    Step "registering Windows Task Scheduler service"
    $installArgs = @("install")
    if ($EnableTelegram) { $installArgs += "--enable-telegram" }
    & $ExePath @installArgs
    if ($LASTEXITCODE -ne 0) {
        Warn "we-forgectl install returned exit $LASTEXITCODE"
        Warn "you can re-run manually: we-forgectl install"
    } else {
        OK "service installed"
    }
} else {
    Warn "skipping service install (-NoServiceInstall)"
    Warn "register manually later: we-forgectl install"
}

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

Write-Host ""
Write-Host "Done." -ForegroundColor Green
Write-Host ""
Write-Host "Useful next steps:" -ForegroundColor White
Write-Host "  - Open a NEW PowerShell window (so PATH takes effect everywhere)"
Write-Host "  - we-forgectl status               # check service + interval + next tick"
Write-Host "  - we-forgectl set-interval 60      # tick every 60 minutes"
Write-Host "  - we-forgectl logs                 # recent tick output"
if (-not $EnableTelegram) {
    Write-Host "  - we-forgectl install --enable-telegram  # opt-in Telegram bot"
}
Write-Host ""
