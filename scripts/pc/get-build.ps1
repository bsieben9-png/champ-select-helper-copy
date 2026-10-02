# Put the newest Champ Select Helper build in %USERPROFILE%\ChampSelectHelper\
#   1) If Rust is installed: build it locally from this repo.
#   2) Otherwise: download the latest GitHub Actions build (needs GitHub CLI: winget install GitHub.cli; gh auth login).
param([switch]$Download, [switch]$Local)
$ErrorActionPreference = "Stop"
$repo = (Resolve-Path "$PSScriptRoot\..\..").Path
$dest = Join-Path $env:USERPROFILE "ChampSelectHelper"
$stamp = Get-Date -Format "yyyy-MM-dd_HH-mm"
$out = Join-Path $dest $stamp
New-Item -ItemType Directory -Force $out | Out-Null

$haveRust = [bool](Get-Command cargo -ErrorAction SilentlyContinue)
if ($Local -or ($haveRust -and -not $Download)) {
    Write-Host "Building locally (this takes a while the first time)..." -ForegroundColor Yellow
    Set-Location $repo
    npm ci
    npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw "Local build failed." }
    Copy-Item "$repo\src-tauri\target\release\champ-select-helper.exe" $out
    Get-ChildItem "$repo\src-tauri\target\release\bundle" -Recurse -Include *.exe, *.msi |
        Copy-Item -Destination $out
} else {
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw "GitHub CLI not found. Run: winget install GitHub.cli   then: gh auth login"
    }
    Write-Host "Downloading the latest GitHub build..." -ForegroundColor Yellow
    $id = gh run list --repo bsieben9-png/champ-select-helper --workflow build.yml --branch main --status success -L 1 --json databaseId -q ".[0].databaseId"
    if (-not $id) { throw "No successful build found on GitHub yet." }
    gh run download $id --repo bsieben9-png/champ-select-helper -D $out
}

# Keep a stable "latest" copy too.
$latest = Join-Path $dest "latest"
if (Test-Path $latest) { Remove-Item $latest -Recurse -Force }
Copy-Item $out $latest -Recurse
Write-Host "Build ready in $latest" -ForegroundColor Green
Invoke-Item $latest
