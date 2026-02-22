# =============================================================================
# D2R Suite — Unified Installer
# =============================================================================
# Builds + installs both native messaging hosts and Chrome extension
#   - Vision Agent  → com.chromium.display.calibration (chrome_helper.exe)
#   - Map Helper    → com.d2vision.map (chrome_map_helper.exe)
#   - Chrome Extension (Display Calibration Helper)
#
# Usage:
#   .\install.ps1                            # Install (prompts for extension ID)
#   .\install.ps1 -ExtensionId <id>          # Install with known extension ID
#   .\install.ps1 -SkipBuild                 # Skip cargo build, use existing binaries
#   .\install.ps1 -Uninstall                 # Remove everything
#   .\install.ps1 -ExtensionOnly             # Just open Chrome to load extension
# =============================================================================

param(
    [string]$ExtensionId = "",
    [string]$VisionInstallPath = "$env:ProgramData\DisplayCalibration",
    [string]$MapInstallPath = "$env:ProgramData\Google\Chrome\NativeMessagingHosts",
    [switch]$Uninstall,
    [switch]$SkipBuild,
    [switch]$ExtensionOnly
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

# Host identifiers
$VisionHostName = "com.chromium.display.calibration"
$MapHostName = "com.d2vision.map"
$VisionExe = "chrome_helper.exe"
$MapExe = "chrome_map_helper.exe"

# ---- Helpers ----

function Write-Banner($text) {
    $line = "=" * 55
    Write-Host ""
    Write-Host $line -ForegroundColor Cyan
    Write-Host " $text" -ForegroundColor Cyan
    Write-Host $line -ForegroundColor Cyan
    Write-Host ""
}

function Write-Step($msg) {
    Write-Host "[+] $msg" -ForegroundColor Green
}

function Write-Warn($msg) {
    Write-Host "[!] $msg" -ForegroundColor Yellow
}

function Write-Err($msg) {
    Write-Host "[-] $msg" -ForegroundColor Red
}

function Write-Info($msg) {
    Write-Host "    $msg" -ForegroundColor Gray
}

function Register-Host($hostName, $manifestPath) {
    # Chrome
    $chromePath = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
    if (-not (Test-Path $chromePath)) {
        New-Item -Path $chromePath -Force | Out-Null
    }
    Set-ItemProperty -Path $chromePath -Name "(Default)" -Value $manifestPath

    # Edge
    $edgePath = "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
    if (-not (Test-Path $edgePath)) {
        New-Item -Path $edgePath -Force | Out-Null
    }
    Set-ItemProperty -Path $edgePath -Name "(Default)" -Value $manifestPath
}

function Unregister-Host($hostName) {
    @(
        "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName",
        "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
    ) | ForEach-Object {
        if (Test-Path $_) {
            Remove-Item $_ -Recurse -Force
            Write-Step "Removed registry: $_"
        }
    }
}

function Write-Manifest($path, $hostName, $exePath, $extId) {
    $escaped = $exePath -replace '\\', '\\\\'
    @"
{
  "name": "$hostName",
  "description": "Chrome Native Messaging Host",
  "path": "$escaped",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://$extId/"
  ]
}
"@ | Out-File -FilePath $path -Encoding utf8 -Force
}

# =============================================
# UNINSTALL
# =============================================
if ($Uninstall) {
    Write-Banner "D2R Suite — Uninstaller"

    # Vision agent
    Write-Host "Removing Vision Agent..." -ForegroundColor Yellow
    Unregister-Host $VisionHostName
    @("$VisionInstallPath\$VisionExe", "$VisionInstallPath\native_host_manifest.json") | ForEach-Object {
        if (Test-Path $_) { Remove-Item $_ -Force; Write-Step "Removed: $_" }
    }
    if ((Test-Path $VisionInstallPath) -and (Get-ChildItem $VisionInstallPath | Measure-Object).Count -eq 0) {
        Remove-Item $VisionInstallPath -Force
        Write-Step "Removed empty dir: $VisionInstallPath"
    }

    # Map helper
    Write-Host ""
    Write-Host "Removing Map Helper..." -ForegroundColor Yellow
    Unregister-Host $MapHostName
    @("$MapInstallPath\$MapExe", "$MapInstallPath\map_manifest.json") | ForEach-Object {
        if (Test-Path $_) { Remove-Item $_ -Force; Write-Step "Removed: $_" }
    }

    Write-Host ""
    Write-Step "Uninstall complete."
    Write-Warn "Chrome extension must be removed manually from chrome://extensions"
    exit 0
}

# =============================================
# EXTENSION-ONLY MODE
# =============================================
if ($ExtensionOnly) {
    Write-Banner "Chrome Extension Setup"
    $extPath = Join-Path $ScriptDir "extension\chrome_extension"
    if (-not (Test-Path "$extPath\manifest.json")) {
        Write-Err "Extension not found at: $extPath"
        exit 1
    }
    Write-Host "Extension directory: $extPath" -ForegroundColor White
    Write-Host ""
    Write-Host "Steps:" -ForegroundColor Yellow
    Write-Host "  1. Open Chrome -> chrome://extensions" -ForegroundColor White
    Write-Host "  2. Enable 'Developer mode' (top right)" -ForegroundColor White
    Write-Host "  3. Click 'Load unpacked'" -ForegroundColor White
    Write-Host "  4. Select: $extPath" -ForegroundColor Cyan
    Write-Host "  5. Copy the Extension ID shown" -ForegroundColor White
    Write-Host "  6. Run: .\install.ps1 -ExtensionId <your-id>" -ForegroundColor Green
    Write-Host ""

    # Try to open Chrome extensions page
    try { Start-Process "chrome://extensions" } catch {}
    exit 0
}

# =============================================
# FULL INSTALL
# =============================================
Write-Banner "D2R Suite — Unified Installer"

# ---- Step 1: Check extension ID ----
if ([string]::IsNullOrWhiteSpace($ExtensionId)) {
    Write-Warn "No extension ID provided."
    Write-Host ""
    Write-Host "If you haven't loaded the extension yet, run:" -ForegroundColor White
    Write-Host "  .\install.ps1 -ExtensionOnly" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Then re-run with your extension ID:" -ForegroundColor White
    Write-Host "  .\install.ps1 -ExtensionId <your-id>" -ForegroundColor Cyan
    Write-Host ""
    $ExtensionId = Read-Host "Enter extension ID (or press Enter to use placeholder)"
    if ([string]::IsNullOrWhiteSpace($ExtensionId)) {
        $ExtensionId = "EXTENSION_ID_HERE"
        Write-Warn "Using placeholder. Re-run with real ID after loading extension."
    }
}
Write-Info "Extension ID: $ExtensionId"

# ---- Step 2: Build binaries ----
if (-not $SkipBuild) {
    Write-Host ""
    Write-Host "Building binaries (release mode)..." -ForegroundColor Yellow

    # Check for cargo
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Err "cargo not found! Install Rust from https://rustup.rs"
        exit 1
    }

    # Build vision agent
    Write-Host "  Building vision agent..." -ForegroundColor Gray
    Push-Location (Join-Path $ScriptDir "botter")
    try {
        cargo build --release 2>&1 | ForEach-Object { Write-Host "    $_" -ForegroundColor DarkGray }
        if ($LASTEXITCODE -ne 0) { Write-Err "Vision agent build failed!"; exit 1 }
        Write-Step "Vision agent built"
    } finally { Pop-Location }

    # Build map helper
    Write-Host "  Building map helper..." -ForegroundColor Gray
    Push-Location (Join-Path $ScriptDir "maphack")
    try {
        cargo build --release 2>&1 | ForEach-Object { Write-Host "    $_" -ForegroundColor DarkGray }
        if ($LASTEXITCODE -ne 0) { Write-Err "Map helper build failed!"; exit 1 }
        Write-Step "Map helper built"
    } finally { Pop-Location }
} else {
    Write-Warn "Skipping build (--SkipBuild)"
}

# ---- Step 3: Install Vision Agent ----
Write-Host ""
Write-Host "Installing Vision Agent..." -ForegroundColor Yellow

if (-not (Test-Path $VisionInstallPath)) {
    New-Item -ItemType Directory -Path $VisionInstallPath -Force | Out-Null
}

$visionBin = Join-Path $ScriptDir "botter\target\release\d2_vision_agent.exe"
if (Test-Path $visionBin) {
    Copy-Item $visionBin "$VisionInstallPath\$VisionExe" -Force
    Write-Step "Installed $VisionExe -> $VisionInstallPath"
} else {
    Write-Err "$VisionExe binary not found at: $visionBin"
    Write-Warn "Run without -SkipBuild or build manually: cd botter && cargo build --release"
}

$visionManifest = "$VisionInstallPath\native_host_manifest.json"
Write-Manifest $visionManifest $VisionHostName "$VisionInstallPath\$VisionExe" $ExtensionId
Register-Host $VisionHostName $visionManifest
Write-Step "Registered $VisionHostName (Chrome + Edge)"

# ---- Step 4: Install Map Helper ----
Write-Host ""
Write-Host "Installing Map Helper..." -ForegroundColor Yellow

if (-not (Test-Path $MapInstallPath)) {
    New-Item -ItemType Directory -Path $MapInstallPath -Force | Out-Null
}

$mapBin = Join-Path $ScriptDir "maphack\target\release\chrome_map_helper.exe"
if (Test-Path $mapBin) {
    Copy-Item $mapBin "$MapInstallPath\$MapExe" -Force
    Write-Step "Installed $MapExe -> $MapInstallPath"
} else {
    Write-Err "$MapExe binary not found at: $mapBin"
    Write-Warn "Run without -SkipBuild or build manually: cd maphack && cargo build --release"
}

$mapManifest = "$MapInstallPath\map_manifest.json"
Write-Manifest $mapManifest $MapHostName "$MapInstallPath\$MapExe" $ExtensionId
Register-Host $MapHostName $mapManifest
Write-Step "Registered $MapHostName (Chrome + Edge)"

# ---- Step 5: Copy config template ----
Write-Host ""
Write-Host "Setting up configs..." -ForegroundColor Yellow

$configDir = Join-Path $VisionInstallPath "configs"
if (-not (Test-Path $configDir)) {
    New-Item -ItemType Directory -Path $configDir -Force | Out-Null
}

$sourceConfigs = Join-Path $ScriptDir "botter\configs"
if (Test-Path $sourceConfigs) {
    Get-ChildItem "$sourceConfigs\*.yaml" | ForEach-Object {
        Copy-Item $_.FullName $configDir -Force
        Write-Info "  Copied: $($_.Name)"
    }
    Write-Step "Configs installed to $configDir"
} else {
    Write-Warn "No config directory found at $sourceConfigs"
}

# ---- Step 6: Verify ----
Write-Host ""
Write-Banner "Installation Complete"

Write-Host "Registered Native Messaging Hosts:" -ForegroundColor White
$hosts = @(
    @{ Name = $VisionHostName; Label = "Vision Agent" },
    @{ Name = $MapHostName;    Label = "Map Helper"   }
)
foreach ($h in $hosts) {
    $reg = Get-ItemProperty "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$($h.Name)" -ErrorAction SilentlyContinue
    if ($reg) {
        Write-Host "  [OK] $($h.Label) ($($h.Name))" -ForegroundColor Green
    } else {
        Write-Host "  [--] $($h.Label) ($($h.Name)) - NOT FOUND" -ForegroundColor Red
    }
}

Write-Host ""
Write-Host "Extension:" -ForegroundColor White
$extPath = Join-Path $ScriptDir "extension\chrome_extension"
Write-Host "  Path: $extPath" -ForegroundColor Gray
if ($ExtensionId -eq "EXTENSION_ID_HERE") {
    Write-Host "  [!!] Extension ID is placeholder — update with real ID" -ForegroundColor Yellow
} else {
    Write-Host "  [OK] Extension ID: $ExtensionId" -ForegroundColor Green
}

Write-Host ""
Write-Host "Configs:" -ForegroundColor White
Write-Host "  $configDir" -ForegroundColor Gray
if (Test-Path $configDir) {
    Get-ChildItem "$configDir\*.yaml" | ForEach-Object {
        Write-Host "    - $($_.Name)" -ForegroundColor Gray
    }
}

Write-Host ""
Write-Host "Quick Start:" -ForegroundColor Yellow
Write-Host "  1. Load extension in Chrome (if not done):" -ForegroundColor White
Write-Host "     chrome://extensions -> Load unpacked -> $extPath" -ForegroundColor Cyan
Write-Host "  2. Copy your character config to $configDir" -ForegroundColor White
Write-Host "  3. Launch D2R and start a game" -ForegroundColor White
Write-Host "  4. The extension connects automatically" -ForegroundColor White
Write-Host ""
