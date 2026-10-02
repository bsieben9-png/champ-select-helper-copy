# Resume the Champ Select Helper work with Claude Code on this PC.
#   -Mode Interactive  : opens a Claude Code window with the "continue" prompt (you can watch/steer it)
#   -Mode Unattended   : runs Claude headless in the background, logs to logs\, pushes when done
param(
    [ValidateSet("Interactive", "Unattended")]
    [string]$Mode = "Interactive"
)
$ErrorActionPreference = "Stop"
$repo = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $repo

Write-Host "Champ Select Helper: resuming in $repo ($Mode)" -ForegroundColor Yellow
git pull --ff-only origin main
if ($LASTEXITCODE -ne 0) { Write-Warning "git pull failed - continuing with the local copy." }

if (-not (Get-Command claude -ErrorAction SilentlyContinue)) {
    Write-Error "The 'claude' command isn't installed. Run scripts\pc\setup-pc.ps1 first."
}

$prompt = Get-Content "$PSScriptRoot\resume-prompt.txt" -Raw

if ($Mode -eq "Interactive") {
    # New window that stays open, with Claude Code started on the prompt.
    $promptFile = "$PSScriptRoot\resume-prompt.txt"
    $cmd = "Set-Location '$repo'; claude (Get-Content '$promptFile' -Raw)"
    Start-Process powershell -ArgumentList "-NoExit", "-ExecutionPolicy", "Bypass", "-Command", $cmd
} else {
    $logDir = Join-Path $repo "logs"
    New-Item -ItemType Directory -Force $logDir | Out-Null
    $log = Join-Path $logDir ("resume-" + (Get-Date -Format "yyyy-MM-dd_HH-mm") + ".log")
    Write-Host "Running unattended. Log: $log"
    claude -p $prompt --dangerously-skip-permissions *> $log
    git push origin main *>> $log
    # Grab the newest build into %USERPROFILE%\ChampSelectHelper when done.
    & "$PSScriptRoot\get-build.ps1" *>> $log
    Write-Host "Done. See $log"
}
