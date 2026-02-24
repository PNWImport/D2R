# =============================================================================
# KZB Suite - Uninstaller / Cleanup
# =============================================================================
# Removes everything installed by install.ps1:
#   - Native messaging host binaries + manifests
#   - Registry entries (Chrome + Edge) for both hosts
#   - Config files and install directories
#   - Network latency tweaks (TcpNoDelay / TcpAckFrequency)
#
# Usage:
#   .\uninstall.ps1                  # Interactive (asks before deleting configs)
#   .\uninstall.ps1 -Force           # Delete everything without prompting
#   .\uninstall.ps1 -KeepConfigs     # Remove binaries/registry but leave configs
#   .\uninstall.ps1 -KeepNetwork     # Skip reverting network tweaks
#   .\uninstall.ps1 -DryRun          # Show what would be removed, change nothing
# =============================================================================

param(
    [switch]$Force,
    [switch]$KeepConfigs,
    [switch]$KeepNetwork,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

# ---- Paths (must match install.ps1 defaults) ----
$VisionInstallPath = "$env:ProgramData\DisplayCalibration"
$MapInstallPath    = "$env:ProgramData\Google\Chrome\NativeMessagingHosts"
$ManifestPath      = "$env:USERPROFILE\KZB\native-hosts"
$VisionExe         = "chrome_helper.exe"
$MapExe            = "chrome_map_helper.exe"
$VisionHostName    = "com.chromium.display.calibration"
$MapHostName       = "com.chromium.canvas.accessibility"

# ---- Helpers ----
$removed  = [System.Collections.Generic.List[string]]::new()
$skipped  = [System.Collections.Generic.List[string]]::new()
$errors   = [System.Collections.Generic.List[string]]::new()

function Write-Banner($text) {
    $line = "=" * 55
    Write-Host ""
    Write-Host $line -ForegroundColor Cyan
    Write-Host " $text" -ForegroundColor Cyan
    Write-Host $line -ForegroundColor Cyan
    Write-Host ""
}

function Write-Step($msg)  { Write-Host "[+] $msg" -ForegroundColor Green  }
function Write-Warn($msg)  { Write-Host "[!] $msg" -ForegroundColor Yellow }
function Write-Err($msg)   { Write-Host "[-] $msg" -ForegroundColor Red    }
function Write-Info($msg)  { Write-Host "    $msg" -ForegroundColor Gray   }
function Write-Dry($msg)   { Write-Host "[DRY] $msg" -ForegroundColor Magenta }

function Remove-ItemSafe($path) {
    if (-not (Test-Path $path)) { return }
    if ($DryRun) {
        Write-Dry "Would remove: $path"
        $script:removed.Add("(dry) $path")
        return
    }
    try {
        Remove-Item $path -Recurse -Force -ErrorAction Stop
        Write-Step "Removed: $path"
        $script:removed.Add($path)
    } catch {
        Write-Err "Failed to remove $path — $_"
        $script:errors.Add($path)
    }
}

function Remove-RegKeySafe($path) {
    if (-not (Test-Path $path)) { return }
    if ($DryRun) {
        Write-Dry "Would delete registry key: $path"
        $script:removed.Add("(dry) REG: $path")
        return
    }
    try {
        Remove-Item $path -Recurse -Force -ErrorAction Stop
        Write-Step "Removed registry key: $path"
        $script:removed.Add("REG: $path")
    } catch {
        Write-Err "Failed to remove registry key $path — $_"
        $script:errors.Add("REG: $path")
    }
}

function Stop-ProcessSafe($name) {
    $procs = Get-Process -Name $name -ErrorAction SilentlyContinue
    if (-not $procs) { return }
    foreach ($p in $procs) {
        if ($DryRun) {
            Write-Dry "Would stop process: $name (PID $($p.Id))"
            continue
        }
        try {
            $p | Stop-Process -Force -ErrorAction Stop
            Write-Step "Stopped process: $name (PID $($p.Id))"
        } catch {
            Write-Warn "Could not stop $name (PID $($p.Id)) — $_"
        }
    }
}

# ---- Admin check ----
$isAdmin = ([Security.Principal.WindowsPrincipal] `
    [Security.Principal.WindowsIdentity]::GetCurrent() `
).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $isAdmin) {
    Write-Host ""
    Write-Host "[!] Not running as Administrator." -ForegroundColor Yellow
    Write-Host "    Some steps will be skipped (ProgramData files, network revert)." -ForegroundColor Yellow
    Write-Host "    For a full cleanup, right-click PowerShell -> Run as Administrator." -ForegroundColor Cyan
    Write-Host ""
}

if ($DryRun) {
    Write-Host ""
    Write-Host "*** DRY RUN — nothing will be changed ***" -ForegroundColor Magenta
}

Write-Banner "KZB Suite - Uninstaller"

# =============================================
# 1. Kill running processes
# =============================================
Write-Host "Stopping running processes..." -ForegroundColor Yellow
Stop-ProcessSafe ($VisionExe -replace '\.exe$', '')
Stop-ProcessSafe ($MapExe    -replace '\.exe$', '')
Start-Sleep -Milliseconds 500

# =============================================
# 2. Unregister native messaging hosts (registry)
# =============================================
Write-Host ""
Write-Host "Removing registry entries..." -ForegroundColor Yellow

$regRoots = @(
    "HKCU:\Software\Google\Chrome\NativeMessagingHosts",
    "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts"
)
foreach ($root in $regRoots) {
    Remove-RegKeySafe "$root\$VisionHostName"
    Remove-RegKeySafe "$root\$MapHostName"
}

# =============================================
# 3. Remove vision agent files
# =============================================
Write-Host ""
Write-Host "Removing Vision Agent files..." -ForegroundColor Yellow

# Binary
Remove-ItemSafe "$VisionInstallPath\$VisionExe"

# Manifest written to install dir (legacy installs)
Remove-ItemSafe "$VisionInstallPath\native_host_manifest.json"

# Configs — ask unless -Force or -KeepConfigs
$configDir = "$VisionInstallPath\configs"
if (Test-Path $configDir) {
    if ($KeepConfigs) {
        Write-Warn "Keeping configs (-KeepConfigs): $configDir"
        $skipped.Add($configDir)
    } elseif ($Force -or $DryRun) {
        Remove-ItemSafe $configDir
    } else {
        Write-Host ""
        Write-Host "  Config directory found: $configDir" -ForegroundColor White
        $answer = Read-Host "  Delete configs? [y/N]"
        if ($answer -match '^[Yy]') {
            Remove-ItemSafe $configDir
        } else {
            Write-Warn "Keeping configs: $configDir"
            $skipped.Add($configDir)
        }
    }
}

# Remove install directory if now empty
if (-not $DryRun -and (Test-Path $VisionInstallPath)) {
    $remaining = Get-ChildItem $VisionInstallPath -ErrorAction SilentlyContinue
    if (-not $remaining) {
        Remove-ItemSafe $VisionInstallPath
    } else {
        Write-Info "Left non-empty directory: $VisionInstallPath"
        Write-Info "Remaining items:"
        $remaining | ForEach-Object { Write-Info "  $_" }
    }
}

# =============================================
# 4. Remove map helper files
# =============================================
Write-Host ""
Write-Host "Removing Map Helper files..." -ForegroundColor Yellow

Remove-ItemSafe "$MapInstallPath\$MapExe"
Remove-ItemSafe "$MapInstallPath\map_manifest.json"

# =============================================
# 5. Remove manifest files from user profile
# =============================================
Write-Host ""
Write-Host "Removing manifest files..." -ForegroundColor Yellow

Remove-ItemSafe "$ManifestPath\native_host_manifest.json"
Remove-ItemSafe "$ManifestPath\map_manifest.json"

# Remove manifest dir if empty
if (-not $DryRun -and (Test-Path $ManifestPath)) {
    $remaining = Get-ChildItem $ManifestPath -ErrorAction SilentlyContinue
    if (-not $remaining) {
        Remove-ItemSafe $ManifestPath
    }
}

# =============================================
# 6. Revert network tweaks
# =============================================
if (-not $KeepNetwork) {
    Write-Host ""
    Write-Host "Reverting network latency tweaks..." -ForegroundColor Yellow

    if (-not $isAdmin) {
        Write-Warn "Skipping — requires Administrator to write HKLM."
        $skipped.Add("Network revert (no admin)")
    } else {
        $ifacesRoot = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces"
        $reverted = 0
        Get-ChildItem $ifacesRoot -ErrorAction SilentlyContinue | ForEach-Object {
            $p = $_.PSPath
            $nd = (Get-ItemProperty $p -Name "TcpNoDelay"      -ErrorAction SilentlyContinue).TcpNoDelay
            $af = (Get-ItemProperty $p -Name "TcpAckFrequency" -ErrorAction SilentlyContinue).TcpAckFrequency
            if ($nd -eq 1 -or $af -eq 1) {
                if ($DryRun) {
                    Write-Dry "Would revert TcpNoDelay/TcpAckFrequency on $($_.Name)"
                } else {
                    Remove-ItemProperty -Path $p -Name "TcpNoDelay"      -ErrorAction SilentlyContinue
                    Remove-ItemProperty -Path $p -Name "TcpAckFrequency" -ErrorAction SilentlyContinue
                }
                $reverted++
            }
        }
        if ($reverted -gt 0) {
            if (-not $DryRun) { Write-Step "Reverted network tweaks on $reverted interface(s)" }
            else               { Write-Dry  "Would revert $reverted interface(s)" }
        } else {
            Write-Info "No network tweaks found to revert."
        }
    }
} else {
    Write-Warn "Skipping network revert (-KeepNetwork)"
}

# =============================================
# 7. Summary
# =============================================
Write-Banner "Cleanup Summary"

if ($removed.Count -gt 0) {
    Write-Host "Removed ($($removed.Count)):" -ForegroundColor Green
    $removed | ForEach-Object { Write-Host "  $_" -ForegroundColor Gray }
}

if ($skipped.Count -gt 0) {
    Write-Host ""
    Write-Host "Skipped / kept ($($skipped.Count)):" -ForegroundColor Yellow
    $skipped | ForEach-Object { Write-Host "  $_" -ForegroundColor Gray }
}

if ($errors.Count -gt 0) {
    Write-Host ""
    Write-Host "Errors ($($errors.Count)):" -ForegroundColor Red
    $errors | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
}

Write-Host ""
Write-Host "Manual step required:" -ForegroundColor Yellow
Write-Host "  Remove the Chrome extension at chrome://extensions" -ForegroundColor White
Write-Host "  (Chrome doesn't allow extensions to be removed via script)" -ForegroundColor Gray
Write-Host ""

if ($DryRun) {
    Write-Host "*** DRY RUN complete — no changes were made ***" -ForegroundColor Magenta
} elseif ($errors.Count -eq 0) {
    Write-Step "Uninstall complete."
} else {
    Write-Warn "Uninstall finished with $($errors.Count) error(s). See above."
}
