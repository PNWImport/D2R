# =============================================================================
# KZB Map Helper - Chrome Native Messaging Host Installer
# =============================================================================
# Same disguise trick as the vision agent:
#   - Installs alongside chrome_helper.exe
#   - Registered as "com.chromium.canvas.accessibility"
#   - Extension connects to BOTH hosts
# Run as Administrator
# =============================================================================

param(
    [string]$HostPath = "$env:ProgramData\Google\Chrome\NativeMessagingHosts",
    [string]$ExtensionId = "REPLACE_WITH_YOUR_EXTENSION_ID",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$HostName = "com.chromium.canvas.accessibility"
$ExeName = "chrome_map_helper.exe"

function Write-Status($color, $msg) {
    Write-Host $msg -ForegroundColor $color
}

# ---- Uninstall ----
if ($Uninstall) {
    Write-Status Cyan "Uninstalling KZB Map Helper..."
    
    @(
        "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$HostName",
        "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$HostName"
    ) | ForEach-Object {
        if (Test-Path $_) {
            Remove-Item $_ -Recurse -Force
            Write-Status Green "  Removed: $_"
        }
    }
    
    @("$HostPath\$ExeName", "$HostPath\map_manifest.json") | ForEach-Object {
        if (Test-Path $_) { Remove-Item $_ -Force; Write-Status Green "  Removed: $_" }
    }
    
    Write-Status Green "`nMap Helper uninstalled."
    exit 0
}

# ---- Install ----
Write-Status Cyan "==========================================="
Write-Status Cyan " KZB Map Helper Installer"
Write-Status Cyan "==========================================="
Write-Status Yellow "Host Name: $HostName"
Write-Status Yellow "Install To: $HostPath"

# Create directory
if (-not (Test-Path $HostPath)) {
    New-Item -ItemType Directory -Path $HostPath -Force | Out-Null
}

# Copy binary (assumes build output is in current or parent dir)
$sourceBin = $null
foreach ($candidate in @(
    ".\target\release\chrome_map_helper.exe",
    "..\target\release\chrome_map_helper.exe",
    ".\chrome_map_helper.exe"
)) {
    if (Test-Path $candidate) { $sourceBin = $candidate; break }
}

if ($sourceBin) {
    Copy-Item $sourceBin "$HostPath\$ExeName" -Force
    Write-Status Green "[+] Copied $ExeName"
} else {
    Write-Status Red "[-] chrome_map_helper.exe not found! Build first with: cargo build --release"
    Write-Status Yellow "    Continuing with manifest-only install..."
}

# Create native messaging manifest
$manifestPath = "$HostPath\map_manifest.json"
$escapedPath = "$HostPath\$ExeName" -replace '\\', '\\\\'
$manifest = @"
{
  "name": "$HostName",
  "description": "Chrome Map Rendering Helper",
  "path": "$escapedPath",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://$ExtensionId/"
  ]
}
"@
# Write UTF8 without BOM — Out-File adds a BOM on Windows PS 5.1 which Chrome may reject
[System.IO.File]::WriteAllText($manifestPath, $manifest, [System.Text.UTF8Encoding]::new($false))
Write-Status Green "[+] Created manifest: $manifestPath"

# Register in registry (Chrome + Edge)
$regFile = "$env:TEMP\install_map_host.reg"
$escapedManifest = $manifestPath -replace '\\', '\\\\'
@"
Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\Google\Chrome\NativeMessagingHosts\$HostName]
@="$escapedManifest"

[HKEY_CURRENT_USER\Software\Microsoft\Edge\NativeMessagingHosts\$HostName]
@="$escapedManifest"
"@ | Out-File -FilePath $regFile -Encoding unicode -Force

Start-Process -FilePath "reg" -ArgumentList "import `"$regFile`"" -Wait -NoNewWindow
Remove-Item $regFile -Force -ErrorAction SilentlyContinue
Write-Status Green "[+] Registered in Chrome & Edge registry"

# Verify
Write-Status Cyan ""
Write-Status Green "==========================================="
Write-Status Green " Installation Complete!"
Write-Status Green "==========================================="
Write-Status Yellow ""
Write-Status Yellow "Registered hosts:"

$visionHost = Get-ItemProperty "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.display.calibration" -ErrorAction SilentlyContinue
$mapHost = Get-ItemProperty "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$HostName" -ErrorAction SilentlyContinue

if ($visionHost) { Write-Status Green "  [*] com.chromium.display.calibration (vision)" }
else { Write-Status Red "  [ ] com.chromium.display.calibration (not installed)" }

if ($mapHost) { Write-Status Green "  [*] $HostName (map)" }
else { Write-Status Red "  [ ] $HostName (not installed)" }

Write-Status Yellow ""
Write-Status Yellow "NOTE: Prefer using the unified installer (install.ps1) in the repo root."
Write-Status Yellow "  It handles both hosts, extension detection, and network optimization."
