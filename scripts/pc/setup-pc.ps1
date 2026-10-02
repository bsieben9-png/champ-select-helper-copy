# One-time setup of this PC for the Champ Select Helper project.
# Run from the cloned repo:  double-click scripts\pc\Setup.cmd
# Safe to run again; it skips what's already installed.
param(
    [string]$ResumeAt = "7:51 PM",     # local time for the scheduled resume
    [switch]$SkipBuildTools            # skip Rust + Visual Studio C++ Build Tools (~5 GB)
)
$ErrorActionPreference = "Continue"
$repo = (Resolve-Path "$PSScriptRoot\..\..").Path

function Have($cmd) { [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }
function WingetInstall($id, $extra = @()) {
    Write-Host "Installing $id ..." -ForegroundColor Yellow
    winget install --id $id -e --accept-source-agreements --accept-package-agreements @extra
}
function RefreshPath {
    $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
                [Environment]::GetEnvironmentVariable("Path", "User")
}

Write-Host "== Champ Select Helper PC setup ==" -ForegroundColor Cyan
Write-Host "Repo: $repo"

# 1) Tools
if (-not (Have git))  { WingetInstall "Git.Git" }
if (-not (Have node)) { WingetInstall "OpenJS.NodeJS.LTS" }
if (-not (Have gh))   { WingetInstall "GitHub.cli" }
if (-not $SkipBuildTools) {
    if (-not (Have cargo)) { WingetInstall "Rustlang.Rustup" }
    $vs = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $hasCpp = (Test-Path $vs) -and (& $vs -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
    if (-not $hasCpp) {
        WingetInstall "Microsoft.VisualStudio.2022.BuildTools" @("--override", "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
    }
}
RefreshPath
if (-not (Have claude)) {
    Write-Host "Installing the Claude Code command (claude) ..." -ForegroundColor Yellow
    Invoke-RestMethod https://claude.ai/install.ps1 | Invoke-Expression
    RefreshPath
}

# 2) Project dependencies
Set-Location $repo
git pull --ff-only origin main
if (Have npm) { npm install }

# 3) GitHub CLI login (for downloading builds)
if (Have gh) {
    gh auth status *> $null
    if ($LASTEXITCODE -ne 0) { Write-Host "Log in to GitHub (browser opens):"; gh auth login --web --git-protocol https }
}

# 4) Optional: let Claude work in this project without permission prompts (your choice).
$answer = Read-Host "Let Claude Code run in THIS project without permission prompts? (y/N)"
if ($answer -match '^(y|yes)$') {
    New-Item -ItemType Directory -Force "$repo\.claude" | Out-Null
    Set-Content "$repo\.claude\settings.local.json" '{ "permissions": { "defaultMode": "bypassPermissions" } }'
    Write-Host "Saved .claude\settings.local.json (stays on this PC, not uploaded)."
}

# 5) Scheduled resume
$mode = Read-Host "Scheduled resume mode: [I]nteractive window (recommended) or [U]nattended background? (I/U, or N for none)"
if ($mode -match '^[IiUu]') {
    $m = if ($mode -match '^[Uu]') { "Unattended" } else { "Interactive" }
    $at = [datetime]::Parse($ResumeAt)
    if ($at -lt (Get-Date)) { $at = $at.AddDays(1) }
    $action  = New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$PSScriptRoot\resume.ps1`" -Mode $m"
    $trigger = New-ScheduledTaskTrigger -Once -At $at
    Register-ScheduledTask -TaskName "Champ Select Helper - resume" -Action $action -Trigger $trigger -Force | Out-Null
    Write-Host "Scheduled: $m resume at $at (Task Scheduler > 'Champ Select Helper - resume')." -ForegroundColor Green
}

Write-Host ""
Write-Host "Setup done." -ForegroundColor Green
Write-Host " - Resume now:        double-click scripts\pc\Resume.cmd"
Write-Host " - Get newest app:    double-click scripts\pc\GetBuild.cmd  (-> $env:USERPROFILE\ChampSelectHelper\latest)"
Write-Host " - First time in Claude Code: run 'claude' once and log in with your Claude account."
