# =============================================================================
# D2R Suite - Unified Installer
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
    [string]$ManifestPath = "$env:USERPROFILE\D2R\native-hosts",
    [switch]$Uninstall,
    [switch]$SkipBuild,
    [switch]$ExtensionOnly,
    [switch]$SkipNetworkOptimize
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

# ---- Early Admin Check ----
# Writing to C:\ProgramData and HKLM registry requires Administrator.
# Fail fast with a clear message instead of dying mid-install.
$isAdmin = ([Security.Principal.WindowsPrincipal] `
    [Security.Principal.WindowsIdentity]::GetCurrent() `
).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $isAdmin -and -not $ExtensionOnly) {
    if ($env:D2R_FORCE_INSTALL -eq "1") {
        Write-Host "[!] WARNING: Running without Administrator. Some steps may fail." -ForegroundColor Yellow
    } else {
        Write-Host "[-] This installer requires Administrator privileges." -ForegroundColor Red
        Write-Host "    Right-click PowerShell -> 'Run as Administrator'" -ForegroundColor Yellow
        Write-Host "    Then re-run: .\install.ps1 $($args -join ' ')" -ForegroundColor Cyan
        Write-Host "    Or set D2R_FORCE_INSTALL=1 to try anyway (may fail on ProgramData writes)" -ForegroundColor Gray
        exit 1
    }
}

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
    $escaped = $exePath -replace '\\', '\\'
    $json = @"
{
  "name": "$hostName",
  "description": "Chrome Native Messaging Host",
  "path": "$escaped",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://$extId/"
  ]
}
"@
    # Use .NET to write UTF8 without BOM — avoids PowerShell double-escaping
    [System.IO.File]::WriteAllText($path, $json, [System.Text.UTF8Encoding]::new($false))
}

# =============================================
# UNINSTALL
# =============================================
if ($Uninstall) {
    Write-Banner "D2R Suite - Uninstaller"

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

    # Revert network optimizations
    Write-Host ""
    Write-Host "Reverting network optimizations..." -ForegroundColor Yellow
    $isAdmin = ([Security.Principal.WindowsPrincipal] `
        [Security.Principal.WindowsIdentity]::GetCurrent() `
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

    if ($isAdmin) {
        $ifacesRoot = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces"
        $reverted = 0
        Get-ChildItem $ifacesRoot -ErrorAction SilentlyContinue | ForEach-Object {
            $ifacePath = $_.PSPath
            $noDelay = (Get-ItemProperty $ifacePath -Name "TcpNoDelay" -ErrorAction SilentlyContinue).TcpNoDelay
            $ackFreq = (Get-ItemProperty $ifacePath -Name "TcpAckFrequency" -ErrorAction SilentlyContinue).TcpAckFrequency
            if ($noDelay -eq 1 -or $ackFreq -eq 1) {
                Remove-ItemProperty -Path $ifacePath -Name "TcpNoDelay" -ErrorAction SilentlyContinue
                Remove-ItemProperty -Path $ifacePath -Name "TcpAckFrequency" -ErrorAction SilentlyContinue
                $reverted++
            }
        }
        if ($reverted -gt 0) {
            Write-Step "Reverted network settings on $reverted interfaces"
        } else {
            Write-Info "No network optimizations found to revert."
        }
    } else {
        Write-Warn "Skipping network revert - requires Administrator."
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
Write-Banner "D2R Suite - Unified Installer"

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
        $env:CARGO_TERM_COLOR = "never"
        & cargo build --release 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) { Write-Err "Vision agent build failed!"; exit 1 }
        Write-Step "Vision agent built"
    } finally { Pop-Location }

    # Build map helper
    Write-Host "  Building map helper..." -ForegroundColor Gray
    Push-Location (Join-Path $ScriptDir "maphack")
    try {
        & cargo build --release 2>&1 | Out-Host
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

$visionBin = Join-Path $ScriptDir "botter\target\release\kzb_vision_agent.exe"
if (Test-Path $visionBin) {
    Copy-Item $visionBin "$VisionInstallPath\$VisionExe" -Force
    Write-Step "Installed $VisionExe -> $VisionInstallPath"
} else {
    Write-Err "$VisionExe binary not found at: $visionBin"
    Write-Warn "Run without -SkipBuild or build manually: cd botter && cargo build --release"
}

# Write manifests to user-writable location (no admin needed)
if (-not (Test-Path $ManifestPath)) {
    New-Item -ItemType Directory -Path $ManifestPath -Force | Out-Null
}
$visionManifest = "$ManifestPath\native_host_manifest.json"
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

$mapManifest = "$ManifestPath\map_manifest.json"
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

# ---- Step 6: Network Latency Optimization (Leatrix-style) ----
# Disables Nagle's algorithm (TcpNoDelay) and forced ACK batching (TcpAckFrequency)
# on all network interfaces. Same principle as the classic Leatrix Latency Fix:
#   - Nagle's algorithm batches small TCP packets → adds 40-200ms delay
#   - ACK frequency batching delays acknowledgements → adds 200ms round-trip
#   - For D2R online play, these tweaks can reduce rubberbanding/input lag
#   - For offline/single-player: no effect (no TCP traffic), but harmless
#
# Requires Administrator privileges to write to HKLM.
# Pass -SkipNetworkOptimize to skip this step.

if (-not $SkipNetworkOptimize) {
    Write-Host ""
    Write-Host "Applying network latency optimizations (Leatrix-style)..." -ForegroundColor Yellow

    $isAdmin = ([Security.Principal.WindowsPrincipal] `
        [Security.Principal.WindowsIdentity]::GetCurrent() `
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

    if (-not $isAdmin) {
        Write-Warn "Skipping network optimization - requires Administrator."
        Write-Info "Re-run installer as Admin, or run manually:"
        Write-Info "  Set-ItemProperty -Path 'HKLM:\SYSTEM\...\Interfaces\{guid}' -Name TcpNoDelay -Value 1"
        Write-Info "  (or pass -SkipNetworkOptimize to suppress this message)"
    } else {
        $ifacesRoot = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces"
        $interfaces = Get-ChildItem $ifacesRoot -ErrorAction SilentlyContinue
        $applied = 0
        $skipped = 0

        foreach ($iface in $interfaces) {
            $ifacePath = $iface.PSPath
            $guid = Split-Path $iface.Name -Leaf

            # Only apply to interfaces that have an IP address configured
            $ipAddr = (Get-ItemProperty $ifacePath -Name "DhcpIPAddress" -ErrorAction SilentlyContinue).DhcpIPAddress
            if (-not $ipAddr) {
                $ipAddr = (Get-ItemProperty $ifacePath -Name "IPAddress" -ErrorAction SilentlyContinue).IPAddress
            }
            if (-not $ipAddr -or $ipAddr -eq "0.0.0.0" -or $ipAddr -eq "") {
                $skipped++
                continue
            }

            # TcpNoDelay = 1: Disable Nagle's algorithm
            # Sends TCP packets immediately instead of batching small writes.
            # Effect: ~40-200ms reduction on small packet games (D2R uses small state packets)
            Set-ItemProperty -Path $ifacePath -Name "TcpNoDelay" -Value 1 -Type DWord -Force

            # TcpAckFrequency = 1: Acknowledge every TCP packet immediately
            # Default Windows behavior batches ACKs, adding up to 200ms round-trip delay.
            # Effect: Server sees ACKs faster → responds faster → less rubberbanding
            Set-ItemProperty -Path $ifacePath -Name "TcpAckFrequency" -Value 1 -Type DWord -Force

            Write-Info "  Applied: $guid ($ipAddr)"
            $applied++
        }

        if ($applied -gt 0) {
            Write-Step "Network optimized: $applied interfaces - TcpNoDelay=1, TcpAckFrequency=1"
            Write-Info "  $skipped interfaces skipped (no IP assigned)"
            Write-Info "  Reboot recommended for changes to take full effect."
        } else {
            Write-Warn "No active network interfaces found to optimize."
        }
    }
} else {
    Write-Info "Skipping network optimization (SkipNetworkOptimize flag set)"
}

# ---- Step 7: Verify ----
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
    Write-Host "  [!!] Extension ID is placeholder - update with real ID" -ForegroundColor Yellow
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
Write-Host "Network Optimization:" -ForegroundColor White
if (-not $SkipNetworkOptimize) {
    $isAdmin = ([Security.Principal.WindowsPrincipal] `
        [Security.Principal.WindowsIdentity]::GetCurrent() `
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if ($isAdmin) {
        $ifacesRoot = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces"
        $optimized = 0
        Get-ChildItem $ifacesRoot -ErrorAction SilentlyContinue | ForEach-Object {
            $nd = (Get-ItemProperty $_.PSPath -Name "TcpNoDelay" -ErrorAction SilentlyContinue).TcpNoDelay
            if ($nd -eq 1) { $optimized++ }
        }
        if ($optimized -gt 0) {
            Write-Host "  [OK] Leatrix-style: TcpNoDelay=1, TcpAckFrequency=1 on $optimized interfaces" -ForegroundColor Green
        } else {
            Write-Host "  [--] Not applied (no active interfaces)" -ForegroundColor Yellow
        }
    } else {
        Write-Host "  [--] Skipped (requires Administrator)" -ForegroundColor Yellow
    }
} else {
    Write-Host "  [--] Skipped (-SkipNetworkOptimize)" -ForegroundColor Gray
}

Write-Host ""
Write-Host "Quick Start:" -ForegroundColor Yellow
Write-Host "  1. Load extension in Chrome (if not done):" -ForegroundColor White
Write-Host "     chrome://extensions -> Load unpacked -> $extPath" -ForegroundColor Cyan
Write-Host "  2. Copy your character config to $configDir" -ForegroundColor White
Write-Host "  3. Launch D2R and start a game" -ForegroundColor White
Write-Host "  4. The extension connects automatically" -ForegroundColor White
Write-Host ""
