# Evolith installer for Windows (x64)
# Usage: irm https://raw.githubusercontent.com/thulasiramk-2310/Evolith/main/install.ps1 | iex

$ErrorActionPreference = 'Stop'
$ProgressPreference    = 'SilentlyContinue'

$repo    = 'thulasiramk-2310/Evolith'
$asset   = 'evolith-windows-x64.zip'
$binDir  = Join-Path $env:LOCALAPPDATA 'Evolith\bin'
$exePath = Join-Path $binDir 'evolith.exe'

Write-Host ""
Write-Host "Installing Evolith..." -ForegroundColor Cyan

# ── resolve latest release ────────────────────────────────────────────────────
$apiUrl  = "https://api.github.com/repos/$repo/releases/latest"
$release = Invoke-RestMethod -Uri $apiUrl -Headers @{ 'User-Agent' = 'evolith-installer' }
$tag     = $release.tag_name

if (-not $tag) {
    Write-Error "Could not resolve the latest Evolith release. Check your internet connection and try again."
    exit 1
}

Write-Host "  Version : $tag" -ForegroundColor Gray

# ── download ──────────────────────────────────────────────────────────────────
$downloadUrl = "https://github.com/$repo/releases/download/$tag/$asset"
$tmpBase     = Join-Path $env:TEMP "evolith-install-$(New-Guid)"
$zipPath     = "$tmpBase.zip"
$extractDir  = "$tmpBase-extracted"

Write-Host "  Downloading from GitHub Releases..." -ForegroundColor Gray
Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath

# ── extract ───────────────────────────────────────────────────────────────────
Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

$exe = Get-ChildItem -Recurse -Filter 'evolith.exe' -Path $extractDir | Select-Object -First 1
if (-not $exe) {
    Write-Error "evolith.exe not found in the downloaded archive. Please report this at https://github.com/$repo/issues"
    exit 1
}

# ── install ───────────────────────────────────────────────────────────────────
if (-not (Test-Path $binDir)) {
    New-Item -ItemType Directory -Path $binDir -Force | Out-Null
}
Copy-Item -Path $exe.FullName -Destination $exePath -Force

Write-Host "  Installed to $exePath" -ForegroundColor Gray

# ── add to user PATH (no admin required) ─────────────────────────────────────
$userPath = [System.Environment]::GetEnvironmentVariable('PATH', 'User')
if ($userPath -notlike "*$binDir*") {
    [System.Environment]::SetEnvironmentVariable('PATH', "$binDir;$userPath", 'User')
    $env:PATH = "$binDir;$env:PATH"
    Write-Host "  Added to PATH" -ForegroundColor Gray
}

# ── clean up ──────────────────────────────────────────────────────────────────
Remove-Item $zipPath      -Force          -ErrorAction SilentlyContinue
Remove-Item $extractDir   -Recurse -Force -ErrorAction SilentlyContinue

# ── done ──────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "Evolith $tag installed." -ForegroundColor Green
Write-Host ""
Write-Host "Quick start:" -ForegroundColor White
Write-Host "  cd your-project"
Write-Host "  evolith init"
Write-Host "  evolith collect git"
Write-Host "  evolith features rebuild"
Write-Host "  evolith doctor"
Write-Host ""
Write-Host "If 'evolith' is not found, open a new terminal window to pick up the PATH change."
Write-Host ""
