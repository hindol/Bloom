# Bloom installer for Windows — https://github.com/hindol/Bloom
# Usage: irm https://raw.githubusercontent.com/hindol/Bloom/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "hindol/Bloom"
$InstallDir = if ($env:BLOOM_INSTALL_DIR) { $env:BLOOM_INSTALL_DIR } else { "$HOME\.bloom\bin" }
$Target = "x86_64-pc-windows-msvc"

function Get-LatestVersion {
    $releases = @(Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases?per_page=1")
    return $releases[0].tag_name
}

$Version = Get-LatestVersion
if (-not $Version) {
    Write-Error "Could not determine latest version"
    exit 1
}

$Url = "https://github.com/$Repo/releases/download/$Version/bloom-$Version-$Target.zip"

Write-Host "Installing Bloom $Version ($Target)..."
Write-Host "  from: $Url"
Write-Host "  to:   $InstallDir\bloom.exe"

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$TempDir = Join-Path $env:TEMP "bloom-install-$(Get-Random)"
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

try {
    $ZipPath = Join-Path $TempDir "bloom.zip"
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing
    Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force
    Copy-Item -Path (Join-Path $TempDir "bloom-tui.exe") -Destination (Join-Path $InstallDir "bloom.exe") -Force
} finally {
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "✓ Bloom installed to $InstallDir\bloom.exe" -ForegroundColor Green

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$UserPath", "User")
    Write-Host ""
    Write-Host "Added $InstallDir to your PATH. Restart your terminal to use 'bloom'." -ForegroundColor Yellow
}
