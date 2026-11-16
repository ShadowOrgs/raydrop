# ==============================================================================
# Raydrop Automated GitHub Release & Landing Page Sync Script
# ==============================================================================
# Usage:
#   npm run release
#   or: powershell -ExecutionPolicy Bypass -File scripts/release.ps1
#
# This script:
#   1. Reads version from package.json automatically
#   2. Syncs version across tauri.conf.json, Cargo.toml, Svelte components & Landing (UTF-8 No BOM)
#   3. Builds installer binary for the exact target version
#   4. Commits and pushes git changes & tags to GitHub
#   5. Creates GitHub Release & uploads .msi installer binary
#   6. Landing page (raydrop.web.id) updates in real-time via GitHub API
# ==============================================================================

$ErrorActionPreference = "Stop"

$repoRoot = (Get-Item $PSScriptRoot).Parent.FullName
Set-Location $repoRoot

function Write-Utf8NoBom($path, $text) {
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($path, $text, $utf8NoBom)
}

function Read-Utf8($path) {
    return [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8)
}

# 1. Read Version
$pkgJson = Read-Utf8 "$repoRoot/package.json" | ConvertFrom-Json
$ver = $pkgJson.version
$tag = "v$ver"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Publishing Raydrop $tag Release to GitHub" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 2. Auto-sync version across all project files to prevent build mismatches
Write-Host "Syncing version $ver across all project files..." -ForegroundColor Yellow

# package-lock.json
$pkgLockPath = "$repoRoot/package-lock.json"
if (Test-Path $pkgLockPath) {
    $lockContent = Read-Utf8 $pkgLockPath
    $lockContent = $lockContent -replace '(?m)^  "version":\s*"[^"]+"', "  `"version`": `"$ver`""
    $lockContent = $lockContent -replace '(?m)^      "version":\s*"[^"]+"', "      `"version`": `"$ver`""
    Write-Utf8NoBom $pkgLockPath $lockContent
}

# tauri.conf.json
$tauriConfPath = "$repoRoot/src-tauri/tauri.conf.json"
if (Test-Path $tauriConfPath) {
    $tConf = Read-Utf8 $tauriConfPath
    $tConf = $tConf -replace '"version":\s*"[^"]+"', "`"version`": `"$ver`""
    Write-Utf8NoBom $tauriConfPath $tConf
}

# Cargo.toml
$cargoPath = "$repoRoot/src-tauri/Cargo.toml"
if (Test-Path $cargoPath) {
    $cContent = Read-Utf8 $cargoPath
    $cContent = $cContent -replace '(?m)^version\s*=\s*"[^"]+"', "version = `"$ver`""
    Write-Utf8NoBom $cargoPath $cContent
}

# Header.svelte
$headerPath = "$repoRoot/src/lib/components/Header.svelte"
if (Test-Path $headerPath) {
    $hContent = Read-Utf8 $headerPath
    $hContent = $hContent -replace 'v\d+\.\d+\.\d+', "v$ver"
    Write-Utf8NoBom $headerPath $hContent
}

# TraySettings.svelte
$settingsPath = "$repoRoot/src/lib/components/TraySettings.svelte"
if (Test-Path $settingsPath) {
    $sPath = Read-Utf8 $settingsPath
    $sPath = $sPath -replace 'Current:\s*v\d+\.\d+\.\d+', "Current: v$ver"
    Write-Utf8NoBom $settingsPath $sPath
}

# README.md
$readmePath = "$repoRoot/README.md"
if (Test-Path $readmePath) {
    $rContent = Read-Utf8 $readmePath
    $rContent = $rContent -replace 'Latest Release \(v[\d\.]+\)', "Latest Release ($tag)"
    $rContent = $rContent -replace 'download/v[\d\.]+/Raydrop_[\d\.]+', "download/$tag/Raydrop_${ver}"
    Write-Utf8NoBom $readmePath $rContent
}

# landing/index.html
$landingHtmlPath = "$repoRoot/landing/index.html"
if (Test-Path $landingHtmlPath) {
    $landingContent = Read-Utf8 $landingHtmlPath
    $landingContent = $landingContent -replace 'Raydrop_[\d\.]+_x64_en-US\.msi', "Raydrop_${ver}_x64_en-US.msi"
    $landingContent = $landingContent -replace 'download/v[\d\.]+/Raydrop', "download/$tag/Raydrop"
    $landingContent = $landingContent -replace 'Raydrop v[\d\.]+ Released', "Raydrop $tag Released"
    Write-Utf8NoBom $landingHtmlPath $landingContent
}

# 3. Verify / Compile Artifacts
$msiPath = "$repoRoot/src-tauri/target/release/bundle/msi/Raydrop_${ver}_x64_en-US.msi"

if (-not (Test-Path $msiPath)) {
    Write-Host "Building release binaries for $tag..." -ForegroundColor Yellow
    npm run tauri build
}

if (-not (Test-Path $msiPath)) {
    Write-Error "Build output missing. Expected:`n  $msiPath"
}

# 4. Get GitHub Credentials
$credInput = "protocol=https`nhost=github.com`n`n"
$credOutput = $credInput | git credential fill
$token = ($credOutput | Select-String "^password=(.+)$").Matches.Groups[1].Value

if (-not $token) {
    Write-Error "GitHub personal access token not found in git credentials. Please sign in with git credential-manager."
}

$headers = @{
    "Authorization" = "Bearer $token"
    "Accept"        = "application/vnd.github+json"
    "User-Agent"    = "Raydrop-Release-Automation"
}

$repo = "asepindrak/raydrop"

# 5. Git Commit & Push Tag
Write-Host "Committing project version sync..." -ForegroundColor Yellow
git add -A
git commit -m "release: $tag" --allow-empty
git tag -f $tag
git push origin main
git push origin $tag --force

# 6. Generate Release Notes
$bodyText = @"
## Raydrop $tag Release

<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

### Download (Windows 64-bit / x64)
- **Windows Setup Installer (`.msi`):** \`Raydrop_${ver}_x64_en-US.msi\`

### Features & Highlights in $tag
- **IDM-Style HTTP Multi-Segment Parallel Download Accelerator:** Accelerate direct web downloads by splitting files into 4–8 concurrent chunks using HTTP Range requests. Direct file sparse writing via native OS file handles without temporary file stitching.
- **BitTorrent Swarm Engine & Magnet Deep Linking:** Full BitTorrent peer-to-peer swarm downloading powered by librqbit v9.1.0. Automatic OS protocol association (magnet:), live peer counter, swarm speed metrics, and pause/resume lifecycle controls.
- **Unified URL & Magnet Downloader:** Single modal (+ Add URL / Magnet) automatically detects whether a pasted link is an HTTP/HTTPS URL or a BitTorrent Magnet link.
- **Universal Clipboard Image Fix:** Robust extraction and rendering of base64 clipboard images and screenshots directly in chat cards with interactive copy and download animations.
- **Universal Web Share Over Internet:** Share files with anyone in the world via on-demand encrypted relay tunnel and QR code without requiring the recipient to install Raydrop.
"@

# 7. Create or Get GitHub Release
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/tags/$tag" -Headers $headers -Method Get
    Write-Host "Found existing release: $($release.html_url)" -ForegroundColor Green
} catch {
    $bodyObj = @{
        tag_name   = $tag
        name       = "Raydrop $tag"
        body       = $bodyText
        draft      = $false
        prerelease = $false
    }
    $jsonBody = $bodyObj | ConvertTo-Json -Depth 5
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases" -Headers $headers -Method Post -Body ([System.Text.Encoding]::UTF8.GetBytes($jsonBody)) -ContentType "application/json; charset=utf-8"
    Write-Host "Created new GitHub release: $($release.html_url)" -ForegroundColor Green
}

# Update rich release notes via UTF-8 safe python script
try {
    python "$repoRoot/scripts/update_release_notes.py" $tag
} catch {
    Write-Host "Warning: Could not run update_release_notes.py: $_" -ForegroundColor Yellow
}

# Remove existing assets for idempotency
foreach ($asset in $release.assets) {
    Write-Host "Removing existing asset: $($asset.name)" -ForegroundColor Gray
    Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/assets/$($asset.id)" -Headers $headers -Method Delete
}

# 8. Upload Assets
$uploadUrlBase = $release.upload_url -replace '\{\?name,label\}$', ''
$uploads = @(
    @{ Path = $msiPath; Name = "Raydrop_${ver}_x64_en-US.msi" }
)

foreach ($u in $uploads) {
    Write-Host "Uploading $($u.Name)..." -ForegroundColor Cyan
    $uploadUri = "${uploadUrlBase}?name=$([uri]::EscapeDataString($u.Name))"
    $uploaded = Invoke-RestMethod -Uri $uploadUri -Headers $headers -Method Post -InFile $u.Path -ContentType "application/octet-stream"
    Write-Host "Uploaded: $($uploaded.browser_download_url)" -ForegroundColor Green
}

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Green
Write-Host " Release $tag is LIVE!" -ForegroundColor Green
Write-Host " GitHub: $($release.html_url)" -ForegroundColor Green
Write-Host " Landing: https://raydrop.web.id (Auto-synced in real-time)" -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green
