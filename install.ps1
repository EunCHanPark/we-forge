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
# 6. Install Claude Code integration files (hooks + global CLAUDE.md)
#
# Fetches we-forge integration files from GitHub raw URLs (no clone needed):
#   - SessionStart hook → ~/.claude/hooks/sessionstart-we-forge.sh
#   - Global CLAUDE.md template → ~/.claude/CLAUDE.md (marker-bounded merge)
#
# These make every new Claude Code session aware of we-forge and inject live
# status into the model's context.
# ---------------------------------------------------------------------------

$ClaudeHome = Join-Path $env:USERPROFILE ".claude"
$HooksDir   = Join-Path $ClaudeHome "hooks"
$RawBase    = "https://raw.githubusercontent.com/EunCHanPark/we-forge/main"

Step "installing Claude Code integration (~/.claude/)"
New-Item -ItemType Directory -Force -Path $HooksDir | Out-Null

# 6a. Hooks (SessionStart + Stop/SubagentStop telemetry)
try {
    foreach ($h in @("sessionstart-we-forge.sh", "stop-telemetry.sh")) {
        $hookDest = Join-Path $HooksDir $h
        Invoke-WebRequest -Uri "$RawBase/hooks/$h" -OutFile $hookDest -UseBasicParsing
        OK "hook installed -> $hookDest"
    }
} catch {
    Warn "hook install skipped: $($_.Exception.Message)"
}

# 6a-2. Learning runtime (tick.sh + normalize.py + redact.sh + data/ skeleton)
#
# Without this block the daemon registers but the tick pipeline can't run:
# tick.sh doesn't exist to be called, normalize.py can't canonicalize events,
# redact.sh can't filter secrets. The install.sh (Mac/Linux) copies these
# from the cloned repo; the native Windows installer fetches them via raw URLs.
try {
    $LearnDir     = Join-Path $ClaudeHome "learning"
    $LearnDataDir = Join-Path $LearnDir "data"
    New-Item -ItemType Directory -Force -Path $LearnDataDir | Out-Null
    foreach ($f in @("tick.sh","normalize.py","redact.sh","settings.snippet.json")) {
        $dest = Join-Path $LearnDir $f
        Invoke-WebRequest -Uri "$RawBase/learning/$f" -OutFile $dest -UseBasicParsing
    }
    foreach ($f in @("events.jsonl","patterns.jsonl","promotion_queue.jsonl","ledger.jsonl","rejected.txt")) {
        $p = Join-Path $LearnDataDir $f
        if (-not (Test-Path $p)) { New-Item -ItemType File -Force -Path $p | Out-Null }
    }
    $stateFile = Join-Path $LearnDataDir "state.json"
    if (-not (Test-Path $stateFile)) {
        Set-Content -Path $stateFile -Value "{}" -Encoding UTF8
    }
    OK "learning runtime installed -> $LearnDir"
} catch {
    Warn "learning runtime install skipped: $($_.Exception.Message)"
}

# 6a-3. Agent definitions (5 sub-agents) — spawned by we-forge tick loop
try {
    $AgentsDir = Join-Path $ClaudeHome "agents"
    New-Item -ItemType Directory -Force -Path $AgentsDir | Out-Null
    foreach ($a in @("we-forge","monitor-sentinel","pattern-detector","quality-auditor","skill-synthesizer")) {
        $dest = Join-Path $AgentsDir "$a.md"
        Invoke-WebRequest -Uri "$RawBase/agents/$a.md" -OutFile $dest -UseBasicParsing
    }
    OK "agent definitions installed -> $AgentsDir"
} catch {
    Warn "agents install skipped: $($_.Exception.Message)"
}

# 6a-4. Slash commands — /skill-report, /watch-and-learn, /dashboard, ...
try {
    $CommandsDir = Join-Path $ClaudeHome "commands"
    New-Item -ItemType Directory -Force -Path $CommandsDir | Out-Null
    foreach ($c in @("watch-and-learn","skill-report","ask-codex","ask-gemini")) {
        try {
            $dest = Join-Path $CommandsDir "$c.md"
            Invoke-WebRequest -Uri "$RawBase/commands/$c.md" -OutFile $dest -UseBasicParsing
        } catch {
            # Some commands may not exist in all versions; tolerate 404
        }
    }
    OK "slash commands installed -> $CommandsDir"
} catch {
    Warn "commands install skipped: $($_.Exception.Message)"
}

# 6b. Global CLAUDE.md (marker-bounded merge)
try {
    $tmpTemplate = Join-Path $env:TEMP "we-forge-global-claude.md"
    Invoke-WebRequest -Uri "$RawBase/home/.claude/CLAUDE.md" -OutFile $tmpTemplate -UseBasicParsing
    $globalClaude = Join-Path $ClaudeHome "CLAUDE.md"
    $markerStart  = "<!-- WE-FORGE-GLOBAL-START -->"
    $markerEnd    = "<!-- WE-FORGE-GLOBAL-END -->"
    $template     = Get-Content $tmpTemplate -Raw

    if (-not (Test-Path $globalClaude)) {
        Set-Content -Path $globalClaude -Value "$markerStart`n$template`n$markerEnd" -Encoding UTF8
        OK "global CLAUDE.md created -> $globalClaude"
    } elseif ((Get-Content $globalClaude -Raw) -like "*$markerStart*") {
        $existing = Get-Content $globalClaude -Raw
        $pattern  = "(?ms)$([regex]::Escape($markerStart)).*?$([regex]::Escape($markerEnd))"
        $newContent = [regex]::Replace($existing, $pattern, "$markerStart`n$template`n$markerEnd")
        $backup = "$globalClaude.bak.$([DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))"
        Copy-Item -Path $globalClaude -Destination $backup -Force
        Set-Content -Path $globalClaude -Value $newContent -Encoding UTF8
        OK "we-forge marker block updated in $globalClaude (backup: $backup)"
    } else {
        $backup = "$globalClaude.bak.$([DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))"
        Copy-Item -Path $globalClaude -Destination $backup -Force
        Add-Content -Path $globalClaude -Value "`n`n$markerStart`n$template`n$markerEnd" -Encoding UTF8
        OK "we-forge marker block appended to $globalClaude (backup: $backup)"
    }
    Remove-Item $tmpTemplate -Force -ErrorAction SilentlyContinue
} catch {
    Warn "global CLAUDE.md install skipped: $($_.Exception.Message)"
}

# 6c. Settings.json hook merge
#
# Primary path uses jq (same logic as install.sh). If jq isn't on PATH,
# we use a pure-PowerShell fallback via ConvertTo-Json / ConvertFrom-Json
# so Windows users don't need to install jq separately.
$SettingsFile = Join-Path $ClaudeHome "settings.json"
$jq = Get-Command jq -ErrorAction SilentlyContinue

function Merge-WeForgeSettingsPS {
    param([string]$Path)
    # Pure-PowerShell merge: same semantics as the jq expression below.
    $existing = if (Test-Path $Path) {
        try { Get-Content $Path -Raw | ConvertFrom-Json -AsHashtable } catch { @{} }
    } else { @{} }
    if (-not $existing) { $existing = @{} }
    if (-not $existing.hooks) { $existing.hooks = @{} }
    if (-not $existing.env)   { $existing.env   = @{} }

    $hookSpecs = @(
        @{event="SessionStart"; cmd="~/.claude/hooks/sessionstart-we-forge.sh"},
        @{event="Stop";         cmd="~/.claude/hooks/stop-telemetry.sh"},
        @{event="SubagentStop"; cmd="~/.claude/hooks/stop-telemetry.sh"}
    )
    foreach ($spec in $hookSpecs) {
        $ev = $spec.event; $cmd = $spec.cmd
        if (-not $existing.hooks[$ev]) { $existing.hooks[$ev] = @() }
        $arr = @($existing.hooks[$ev])
        $emptyMatchers = @($arr | Where-Object { $_.matcher -eq "" -or $null -eq $_.matcher })
        if ($emptyMatchers.Count -eq 0) {
            $arr += @{matcher=""; hooks=@(@{type="command"; command=$cmd})}
        } else {
            foreach ($grp in $emptyMatchers) {
                if (-not $grp.hooks) { $grp.hooks = @() }
                $cmds = @($grp.hooks | ForEach-Object { $_.command })
                if ($cmds -notcontains $cmd) {
                    $grp.hooks = @($grp.hooks) + @(@{type="command"; command=$cmd})
                }
            }
        }
        $existing.hooks[$ev] = $arr
    }

    $existingEccStr = if ($existing.env["ECC_DISABLED_HOOKS"]) { $existing.env["ECC_DISABLED_HOOKS"] } else { "" }
    $additions      = "pre:edit-write:gateguard-fact-force,pre:bash:dispatcher,pre:edit-write:suggest-compact"
    $merged = (($existingEccStr + "," + $additions) -split "," | Where-Object { $_.Length -gt 0 } | Sort-Object -Unique) -join ","
    $existing.env["ECC_DISABLED_HOOKS"] = $merged

    $existing | ConvertTo-Json -Depth 20
}

if (-not (Test-Path $SettingsFile)) {
    $snippet = @'
{
  "env": {
    "ECC_DISABLED_HOOKS": "pre:edit-write:gateguard-fact-force,pre:bash:dispatcher,pre:edit-write:suggest-compact"
  },
  "hooks": {
    "SessionStart": [{ "matcher": "", "hooks": [{ "type": "command", "command": "~/.claude/hooks/sessionstart-we-forge.sh" }] }],
    "Stop":         [{ "matcher": "", "hooks": [{ "type": "command", "command": "~/.claude/hooks/stop-telemetry.sh" }] }],
    "SubagentStop": [{ "matcher": "", "hooks": [{ "type": "command", "command": "~/.claude/hooks/stop-telemetry.sh" }] }]
  }
}
'@
    Set-Content -Path $SettingsFile -Value $snippet -Encoding UTF8
    OK "settings.json created with SessionStart hook + ECC_DISABLED_HOOKS"
} elseif ($jq) {
    $backup = "$SettingsFile.bak.$([DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))"
    Copy-Item -Path $SettingsFile -Destination $backup -Force
    $mergeExpr = @'
.hooks //= {} |
.hooks.SessionStart //= [] |
.hooks.SessionStart |= (
  if (length == 0) then
    [{matcher:"", hooks:[{type:"command", command:"~/.claude/hooks/sessionstart-we-forge.sh"}]}]
  else
    map(
      if (.matcher == "" or .matcher == null) then
        .hooks = ((.hooks // []) | if (map(.command) | index("~/.claude/hooks/sessionstart-we-forge.sh")) then . else . + [{type:"command", command:"~/.claude/hooks/sessionstart-we-forge.sh"}] end)
      else . end
    )
    | if (any(.matcher == "" or .matcher == null)) then . else . + [{matcher:"", hooks:[{type:"command", command:"~/.claude/hooks/sessionstart-we-forge.sh"}]}] end
  end
) |
.hooks.Stop //= [] |
.hooks.Stop |= (
  if (length == 0) then
    [{matcher:"", hooks:[{type:"command", command:"~/.claude/hooks/stop-telemetry.sh"}]}]
  else
    map(if (.matcher == "" or .matcher == null) then .hooks = ((.hooks // []) | if (map(.command) | index("~/.claude/hooks/stop-telemetry.sh")) then . else . + [{type:"command", command:"~/.claude/hooks/stop-telemetry.sh"}] end) else . end)
    | if (any(.matcher == "" or .matcher == null)) then . else . + [{matcher:"", hooks:[{type:"command", command:"~/.claude/hooks/stop-telemetry.sh"}]}] end
  end
) |
.hooks.SubagentStop //= [] |
.hooks.SubagentStop |= (
  if (length == 0) then
    [{matcher:"", hooks:[{type:"command", command:"~/.claude/hooks/stop-telemetry.sh"}]}]
  else
    map(if (.matcher == "" or .matcher == null) then .hooks = ((.hooks // []) | if (map(.command) | index("~/.claude/hooks/stop-telemetry.sh")) then . else . + [{type:"command", command:"~/.claude/hooks/stop-telemetry.sh"}] end) else . end)
    | if (any(.matcher == "" or .matcher == null)) then . else . + [{matcher:"", hooks:[{type:"command", command:"~/.claude/hooks/stop-telemetry.sh"}]}] end
  end
) |
.env //= {} |
.env.ECC_DISABLED_HOOKS = (
  ((.env.ECC_DISABLED_HOOKS // "") + ",pre:edit-write:gateguard-fact-force,pre:bash:dispatcher,pre:edit-write:suggest-compact")
  | split(",") | map(select(length > 0)) | unique | join(",")
)
'@
    $merged = & jq $mergeExpr $SettingsFile
    if ($LASTEXITCODE -eq 0) {
        Set-Content -Path $SettingsFile -Value $merged -Encoding UTF8
        OK "settings.json merged via jq (hooks + ECC_DISABLED_HOOKS). backup: $backup"
    } else {
        Warn "jq merge failed — trying PowerShell fallback"
        try {
            $merged = Merge-WeForgeSettingsPS -Path $SettingsFile
            Set-Content -Path $SettingsFile -Value $merged -Encoding UTF8
            OK "settings.json merged via PowerShell fallback. backup: $backup"
        } catch {
            Warn "PowerShell fallback also failed: $($_.Exception.Message)"
        }
    }
} else {
    # No jq — use PowerShell fallback
    $backup = "$SettingsFile.bak.$([DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))"
    Copy-Item -Path $SettingsFile -Destination $backup -Force
    try {
        $merged = Merge-WeForgeSettingsPS -Path $SettingsFile
        Set-Content -Path $SettingsFile -Value $merged -Encoding UTF8
        OK "settings.json merged via PowerShell (jq not installed). backup: $backup"
        Write-Host "    (install jq for faster merges: winget install jqlang.jq  — optional)" -ForegroundColor DarkGray
    } catch {
        Warn "settings.json merge failed: $($_.Exception.Message)"
        Warn "manual fix: add SessionStart/Stop hooks + ECC_DISABLED_HOOKS env to $SettingsFile"
    }
}

# ---------------------------------------------------------------------------
# 7. Service install (Windows Task Scheduler via Rust manager)
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
