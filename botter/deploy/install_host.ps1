# install_host.ps1 — Run as Administrator
# Installs the native messaging host with stealth-appropriate naming
param(
    [string]$InstallPath = "C:\ProgramData\DisplayCalibration",
    [string]$ExtensionId = "EXTENSION_ID_HERE"
)

Write-Host "Installing Display Calibration Service..." -ForegroundColor Cyan

# Create install directory
if (-not (Test-Path $InstallPath)) {
    New-Item -ItemType Directory -Path $InstallPath -Force | Out-Null
    Write-Host "[+] Created $InstallPath" -ForegroundColor Green
}

# Copy binary
$binarySource = ".\target\release\d2_vision_agent.exe"
if (-not (Test-Path $binarySource)) {
    Write-Host "[-] Binary not found. Run: cargo build --release" -ForegroundColor Red
    exit 1
}
Copy-Item $binarySource "$InstallPath\chrome_helper.exe" -Force
Write-Host "[+] Installed chrome_helper.exe" -ForegroundColor Green

# Generate manifest with actual extension ID
$manifestPath = "$InstallPath\native_host_manifest.json"
$escapedPath = $InstallPath.Replace('\', '\\')
@"
{
  "name": "com.chromium.display.calibration",
  "description": "Display Calibration Native Messaging Host",
  "path": "$escapedPath\\chrome_helper.exe",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://$ExtensionId/"
  ]
}
"@ | Out-File -FilePath $manifestPath -Encoding utf8
Write-Host "[+] Created manifest" -ForegroundColor Green

# Register for Chrome
$regPath = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.display.calibration"
if (-not (Test-Path $regPath)) {
    New-Item -Path $regPath -Force | Out-Null
}
Set-ItemProperty -Path $regPath -Name "(Default)" -Value $manifestPath
Write-Host "[+] Registered for Chrome" -ForegroundColor Green

# Register for Edge (same Chromium base)
$edgePath = "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.chromium.display.calibration"
if (-not (Test-Path $edgePath)) {
    New-Item -Path $edgePath -Force | Out-Null
}
Set-ItemProperty -Path $edgePath -Name "(Default)" -Value $manifestPath
Write-Host "[+] Registered for Edge" -ForegroundColor Green

Write-Host ""
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "  1. Open Chrome -> chrome://extensions"
Write-Host "  2. Enable Developer mode"
Write-Host "  3. Load unpacked -> select chrome_extension folder"
Write-Host "  4. Copy the extension ID"
Write-Host "  5. Re-run: .\install_host.ps1 -ExtensionId <your-id>"
