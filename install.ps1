# =============================================================================
# KZB Suite - Unified Installer
# =============================================================================
# Builds + installs both native messaging hosts and Chrome extension
#   - Vision Agent  → com.chromium.display.calibration (chrome_helper.exe)
#   - Map Helper    → com.chromium.canvas.accessibility (chrome_map_helper.exe)
#   - Chrome Extension (Display Calibration Helper)
#
# Usage:
#   .\install.ps1                            # Full auto: build, detect extension, install
#   .\install.ps1 -ExtensionId <id>          # Install with known extension ID
#   .\install.ps1 -SkipBuild                 # Skip cargo build, use existing binaries
#   .\install.ps1 -Uninstall                 # Remove everything
#   .\install.ps1 -ExtensionOnly             # Just open Chrome to load extension
# =============================================================================

param(
    [string]$ExtensionId = "",
    [string]$VisionInstallPath = "$env:ProgramData\DisplayCalibration",
    [string]$MapInstallPath = "$env:ProgramData\Google\Chrome\NativeMessagingHosts",
    [string]$ManifestPath = "$env:USERPROFILE\KZB\native-hosts",
    [switch]$Uninstall,
    [switch]$SkipBuild,
    [switch]$ExtensionOnly,
    [switch]$SkipNetworkOptimize
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

# ---- OS Detection ----
# This installer targets native Windows. Detect the environment early
# and bail with actionable guidance if we're somewhere else.
$osType = if ($IsLinux)    { "linux"  }
          elseif ($IsMacOS) { "macos"  }
          else              { "windows" }

# Detect WSL (pwsh running inside Windows Subsystem for Linux)
$isWSL = $false
if ($osType -eq "linux" -and (Test-Path "/proc/version")) {
    $procVer = Get-Content "/proc/version" -Raw -ErrorAction SilentlyContinue
    if ($procVer -match "microsoft|WSL") { $isWSL = $true }
}

if ($isWSL) {
    Write-Host "[-] Running inside WSL — this installer must run in native Windows PowerShell." -ForegroundColor Red
    Write-Host ""
    Write-Host "    Open a PowerShell (Admin) window and run:" -ForegroundColor Yellow
    Write-Host "      cd $($ScriptDir -replace '/mnt/c','C:' -replace '/','\')" -ForegroundColor Cyan
    Write-Host "      .\install.ps1" -ForegroundColor Cyan
    exit 1
}

if ($osType -ne "windows") {
    Write-Host "[-] Unsupported OS: $osType" -ForegroundColor Red
    Write-Host "    This installer requires native Windows (registry, ProgramData, Chrome)." -ForegroundColor Yellow
    Write-Host "    The Rust crates (botter, maphack) also require Windows APIs (DXGI, Win32)." -ForegroundColor Gray
    exit 1
}

# ---- Early Admin Check (with UAC self-elevation) ----
# Writing to C:\ProgramData and HKLM registry requires Administrator.
# If not elevated, prompt the user via UAC and re-launch this script.
$isAdmin = ([Security.Principal.WindowsPrincipal] `
    [Security.Principal.WindowsIdentity]::GetCurrent() `
).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $isAdmin -and -not $ExtensionOnly) {
    Write-Host "[!] Administrator privileges required. Requesting elevation..." -ForegroundColor Yellow

    # Rebuild the argument list so the elevated instance keeps all flags
    $relaunchArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$PSCommandPath`"")
    if ($ExtensionId)          { $relaunchArgs += "-ExtensionId `"$ExtensionId`"" }
    if ($SkipBuild)            { $relaunchArgs += "-SkipBuild" }
    if ($SkipNetworkOptimize)  { $relaunchArgs += "-SkipNetworkOptimize" }
    if ($Uninstall)            { $relaunchArgs += "-Uninstall" }

    try {
        Start-Process powershell -Verb RunAs -ArgumentList $relaunchArgs
        Write-Host "    Elevated window opened. You can close this one." -ForegroundColor Gray
        exit 0
    } catch {
        # User declined UAC or RunAs failed
        Write-Host "[-] UAC elevation was declined or failed." -ForegroundColor Red
        Write-Host "    Right-click PowerShell -> 'Run as Administrator'" -ForegroundColor Yellow
        Write-Host "    Then re-run: .\install.ps1" -ForegroundColor Cyan
        exit 1
    }
}

# Host identifiers
$VisionHostName = "com.chromium.display.calibration"
$MapHostName = "com.chromium.canvas.accessibility"
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


function Find-ExtensionId {
    # Auto-detect the KZB extension ID from Chrome's Preferences file.
    # Chrome stores all installed extensions (including unpacked dev-mode)
    # in the Preferences JSON under extensions.settings.
    $profiles = @("Default", "Profile 1", "Profile 2", "Profile 3")
    foreach ($profile in $profiles) {
        $prefsPath = "$env:LOCALAPPDATA\Google\Chrome\User Data\$profile\Preferences"
        if (-not (Test-Path $prefsPath)) { continue }
        try {
            $json = Get-Content $prefsPath -Raw | ConvertFrom-Json
            $settings = $json.extensions.settings
            if (-not $settings) { continue }
            foreach ($prop in $settings.PSObject.Properties) {
                $ext = $prop.Value
                # Match by manifest name
                if ($ext.manifest.name -eq "KZB Control Panel") {
                    return $prop.Name
                }
            }
        } catch {
            # Preferences locked or malformed — skip this profile
            continue
        }
    }
    return $null
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
    Write-Banner "KZB Suite - Uninstaller"

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
    Write-Host "  5. Run: .\install.ps1" -ForegroundColor Green
    Write-Host "     (Extension ID will be auto-detected)" -ForegroundColor Gray
    Write-Host ""

    # Try to open Chrome extensions page
    try { Start-Process "chrome://extensions" } catch {}
    exit 0
}

# =============================================
# FULL INSTALL
# =============================================
Write-Banner "KZB Suite - Unified Installer"

# ---- Step 1: Build binaries (longest step — do first) ----
if (-not $SkipBuild) {
    Write-Host "Building binaries (release mode)..." -ForegroundColor Yellow

    # Check for cargo
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Err "cargo not found! Install Rust from https://rustup.rs"
        exit 1
    }

    # Build vision agent
    # NOTE: cargo writes progress to stderr. Run through cmd /c so PowerShell
    # does not wrap every "Compiling ..." line as a NativeCommandError.
    Write-Host "  Building vision agent..." -ForegroundColor Gray
    Push-Location (Join-Path $ScriptDir "botter")
    try {
        $env:CARGO_TERM_COLOR = "never"
        cmd /c "cargo build --release 2>&1"
        if ($LASTEXITCODE -ne 0) { Write-Err "Vision agent build failed!"; exit 1 }
        Write-Step "Vision agent built"
    } finally { Pop-Location }

    # Build map helper
    Write-Host "  Building map helper..." -ForegroundColor Gray
    Push-Location (Join-Path $ScriptDir "maphack")
    try {
        cmd /c "cargo build --release 2>&1"
        if ($LASTEXITCODE -ne 0) { Write-Err "Map helper build failed!"; exit 1 }
        Write-Step "Map helper built"
    } finally { Pop-Location }
} else {
    Write-Warn "Skipping build (--SkipBuild)"
}

# ---- Step 2: Install Chrome Extension ----
Write-Host ""
Write-Host "Installing Chrome Extension..." -ForegroundColor Yellow

$extPath = Join-Path $ScriptDir "extension\chrome_extension"
if (-not (Test-Path "$extPath\manifest.json")) {
    Write-Err "Extension not found at: $extPath"
    exit 1
}
Write-Step "Extension source: $extPath"

# Find Chrome executable
$chromeExe = $null
foreach ($p in @(
    "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
    "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe"
)) {
    if (Test-Path $p) { $chromeExe = $p; break }
}
if (-not $chromeExe) {
    $chromeExe = (Get-Command chrome -ErrorAction SilentlyContinue).Source
}

# ---- Step 3: Detect Extension ID ----
Write-Host ""
Write-Host "Detecting extension ID..." -ForegroundColor Yellow

if ([string]::IsNullOrWhiteSpace($ExtensionId)) {
    # First, try auto-detect from Chrome Preferences
    $detected = Find-ExtensionId
    if ($detected) {
        $ExtensionId = $detected
        $masked = $ExtensionId.Substring(0, 4) + ("*" * 24) + $ExtensionId.Substring(28, 4)
        Write-Step "Auto-detected extension ID: $masked"
    } else {
        # Extension not loaded yet — load it via --load-extension, then re-detect
        if ($chromeExe) {
            Write-Warn "Extension not found in Chrome. Loading it now..."
            Start-Process $chromeExe -ArgumentList "--load-extension=`"$extPath`"" 2>$null
            Write-Info "Waiting for Chrome to register the extension..."
            # Poll for up to 15 seconds
            for ($i = 0; $i -lt 15; $i++) {
                Start-Sleep -Seconds 1
                $detected = Find-ExtensionId
                if ($detected) {
                    $ExtensionId = $detected
                    $masked = $ExtensionId.Substring(0, 4) + ("*" * 24) + $ExtensionId.Substring(28, 4)
                    Write-Step "Auto-detected extension ID: $masked"
                    break
                }
            }
        }

        # Final fallback: manual entry
        if ([string]::IsNullOrWhiteSpace($ExtensionId)) {
            Write-Warn "Could not auto-detect. Please load the extension manually:"
            Write-Host "  1. Open chrome://extensions" -ForegroundColor Cyan
            Write-Host "  2. Enable Developer mode" -ForegroundColor Cyan
            Write-Host "  3. Load unpacked -> $extPath" -ForegroundColor Cyan
            Write-Host ""
            $ExtensionId = Read-Host "Enter extension ID"
            if ([string]::IsNullOrWhiteSpace($ExtensionId)) {
                Write-Err "Extension ID is required. Cannot continue."
                exit 1
            }
        }
    }
} else {
    Write-Step "Using provided extension ID"
}

# Mask the ID in output for privacy
$maskedId = if ($ExtensionId.Length -ge 32) {
    $ExtensionId.Substring(0, 4) + ("*" * 24) + $ExtensionId.Substring(28, 4)
} else { $ExtensionId }
Write-Step "Extension ID: $maskedId"

# ---- Step 4: Install Vision Agent (copy binary + register) ----
Write-Host ""
Write-Host "Installing Vision Agent..." -ForegroundColor Yellow

if (-not (Test-Path $VisionInstallPath)) {
    New-Item -ItemType Directory -Path $VisionInstallPath -Force | Out-Null
}

$visionBin = Join-Path $ScriptDir "vision\target\release\kzb_vision_agent.exe"
if (Test-Path $visionBin) {
    Copy-Item $visionBin "$VisionInstallPath\$VisionExe" -Force
    Write-Step "Installed $VisionExe -> $VisionInstallPath"
} else {
    Write-Err "$VisionExe binary not found at: $visionBin"
    Write-Warn "Run without -SkipBuild or build manually: cd vision && cargo build --release"
}

# Write manifests to user-writable location (no admin needed)
if (-not (Test-Path $ManifestPath)) {
    New-Item -ItemType Directory -Path $ManifestPath -Force | Out-Null
}
$visionManifest = "$ManifestPath\native_host_manifest.json"
Write-Manifest $visionManifest $VisionHostName "$VisionInstallPath\$VisionExe" $ExtensionId
Register-Host $VisionHostName $visionManifest
Write-Step "Registered $VisionHostName (Chrome + Edge)"

# ---- Step 5: Install Map Helper (copy binary + register) ----
Write-Host ""
Write-Host "Installing Map Helper..." -ForegroundColor Yellow

if (-not (Test-Path $MapInstallPath)) {
    New-Item -ItemType Directory -Path $MapInstallPath -Force | Out-Null
}

$mapBin = Join-Path $ScriptDir "overlay\target\release\chrome_map_helper.exe"
if (Test-Path $mapBin) {
    Copy-Item $mapBin "$MapInstallPath\$MapExe" -Force
    Write-Step "Installed $MapExe -> $MapInstallPath"
} else {
    Write-Err "$MapExe binary not found at: $mapBin"
    Write-Warn "Run without -SkipBuild or build manually: cd overlay && cargo build --release"
}

$mapManifest = "$ManifestPath\map_manifest.json"
Write-Manifest $mapManifest $MapHostName "$MapInstallPath\$MapExe" $ExtensionId
Register-Host $MapHostName $mapManifest
Write-Step "Registered $MapHostName (Chrome + Edge)"

# ---- Step 6: Copy config templates ----
Write-Host ""
Write-Host "Setting up configs..." -ForegroundColor Yellow

$configDir = Join-Path $VisionInstallPath "configs"
if (-not (Test-Path $configDir)) {
    New-Item -ItemType Directory -Path $configDir -Force | Out-Null
}

$sourceConfigs = Join-Path $ScriptDir "vision\configs"
if (Test-Path $sourceConfigs) {
    Get-ChildItem "$sourceConfigs\*.yaml" | ForEach-Object {
        Copy-Item $_.FullName $configDir -Force
        Write-Info "  Copied: $($_.Name)"
    }
    Write-Step "Configs installed to $configDir"
} else {
    Write-Warn "No config directory found at $sourceConfigs"
}

# ---- Step 7: Network Latency Optimization (Leatrix-style) ----
# Disables Nagle's algorithm (TcpNoDelay) and forced ACK batching (TcpAckFrequency)
# on all network interfaces. Same principle as the classic Leatrix Latency Fix:
#   - Nagle's algorithm batches small TCP packets → adds 40-200ms delay
#   - ACK frequency batching delays acknowledgements → adds 200ms round-trip
#   - For online play, these tweaks can reduce rubberbanding/input lag
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
            # Effect: ~40-200ms reduction on small packet games (game uses small state packets)
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

# ---- Step 8: Verify ----
Write-Host ""
Write-Banner "Installation Complete"

Write-Host "Installed Components:" -ForegroundColor White
Write-Host ""

# 1) Chrome Extension
Write-Host "  Chrome Extension (KZB Control Panel v1.7.0):" -ForegroundColor White
if ($ExtensionId -and $ExtensionId -ne "EXTENSION_ID_HERE") {
    Write-Host "    [OK] Loaded in Chrome — ID: $maskedId" -ForegroundColor Green
} else {
    Write-Host "    [!!] Extension ID is placeholder — update with real ID" -ForegroundColor Yellow
}
Write-Host "    Path: $extPath" -ForegroundColor Gray

# 2) Vision Agent
Write-Host ""
Write-Host "  Vision Agent ($VisionHostName):" -ForegroundColor White
$visionReg = Get-ItemProperty "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$VisionHostName" -ErrorAction SilentlyContinue
if ($visionReg) {
    Write-Host "    [OK] Registered (Chrome + Edge)" -ForegroundColor Green
} else {
    Write-Host "    [--] NOT FOUND in registry" -ForegroundColor Red
}
if (Test-Path "$VisionInstallPath\$VisionExe") {
    Write-Host "    Binary: $VisionInstallPath\$VisionExe" -ForegroundColor Gray
} else {
    Write-Host "    [--] Binary missing: $VisionInstallPath\$VisionExe" -ForegroundColor Red
}

# 3) Map Helper
Write-Host ""
Write-Host "  Map Helper ($MapHostName):" -ForegroundColor White
$mapReg = Get-ItemProperty "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$MapHostName" -ErrorAction SilentlyContinue
if ($mapReg) {
    Write-Host "    [OK] Registered (Chrome + Edge)" -ForegroundColor Green
} else {
    Write-Host "    [--] NOT FOUND in registry" -ForegroundColor Red
}
if (Test-Path "$MapInstallPath\$MapExe") {
    Write-Host "    Binary: $MapInstallPath\$MapExe" -ForegroundColor Gray
} else {
    Write-Host "    [--] Binary missing: $MapInstallPath\$MapExe" -ForegroundColor Red
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
Write-Host "  1. Click the KZB extension icon in Chrome toolbar" -ForegroundColor White
Write-Host "     (Opens as a full-screen control panel tab)" -ForegroundColor Gray
Write-Host "  2. Select your character config" -ForegroundColor White
Write-Host "  3. Launch the game and start a session" -ForegroundColor White
Write-Host "  4. The extension connects automatically" -ForegroundColor White
Write-Host ""
