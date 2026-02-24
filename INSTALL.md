# Installation Guide — KZB

Step-by-step setup for Windows 10/11 with Chrome/Edge.

---

## Quick Start (5 minutes)

### Prerequisites
- ✅ Windows 10/11
- ✅ Chrome or Edge browser
- ✅ Game client installed and working (offline/single-player)
- ✅ PowerShell 5.0+ (built-in on Windows 10+)

### Option 1: Automated — 1-Click (Recommended)

```powershell
# 1. Open PowerShell AS ADMINISTRATOR (right-click -> Run as Administrator)
#    The installer needs admin for ProgramData writes and network optimization.

# 2. Navigate to repo root
cd C:\Users\YourName\Downloads\KZB

# 3. Run the unified installer
.\install.ps1

# That's it! The installer will:
#   a. Build both Rust binaries (vision agent + map helper)
#   b. Auto-detect your Chrome extension ID
#      (or launch Chrome to load it if not already loaded)
#   c. Install binaries, manifests, registry entries
#   d. Copy config templates
#   e. Apply network latency optimizations (Leatrix-style)
#
# Optional flags:
#   .\install.ps1 -SkipBuild          # Use pre-built binaries
#   .\install.ps1 -SkipNetworkOptimize # Skip TCP tweaks
#   .\install.ps1 -ExtensionId <id>   # Skip auto-detect
#   .\install.ps1 -ExtensionOnly      # Just show extension load instructions
```

### Option 2: Manual (Fine-grained control)

Skip to "Manual Installation" section below.

---

## Detailed Steps

### Step 1: Build from Source

Requires **Rust toolchain** (from https://rustup.rs/):

```powershell
# Open PowerShell as Administrator
cd C:\Users\YourName\Downloads\KZB

# Build vision agent
cd botter
cargo build --release
# Output: target\release\kzb_vision_agent.exe (3-5 minutes)

# Build map helper
cd ..\maphack
cargo build --release
# Output: target\release\chrome_map_helper.exe (2-3 minutes)

# Test everything works
cd ..\botter
cargo test
# Should see: test result: ok. 294 passed
```

### Step 2: Load Chrome Extension

```
1. Open chrome://extensions
2. Enable "Developer mode" (top-right toggle)
3. Click "Load unpacked"
4. Select: C:\Users\YourName\Downloads\KZB\extension\chrome_extension\
5. Note the Extension ID (blue, under the extension name)
   Example: "abcdefghijklmnopqrstuvwxyz123456"
```

### Step 3: Install Native Messaging Hosts

**As Administrator, in PowerShell:**

```powershell
cd C:\Users\YourName\Downloads\KZB

# Run installer — extension ID is auto-detected from Chrome
.\install.ps1

# Script will:
# ✓ Build vision agent + map helper (Rust release binaries)
# ✓ Auto-detect extension ID from Chrome Preferences
#   (or launch Chrome with --load-extension if not yet loaded)
# ✓ Install native messaging host binaries:
#   - chrome_helper.exe    → C:\ProgramData\DisplayCalibration\
#   - chrome_map_helper.exe → C:\ProgramData\Google\Chrome\NativeMessagingHosts\
# ✓ Write JSON manifests to %USERPROFILE%\KZB\native-hosts\
# ✓ Register hosts in HKCU registry (Chrome + Edge)
# ✓ Copy config templates to C:\ProgramData\DisplayCalibration\configs\
# ✓ Apply Leatrix TCP optimization (TcpNoDelay, TcpAckFrequency)
#
# Optional flags:
#   -SkipBuild              Skip Rust compilation (use existing binaries)
#   -SkipNetworkOptimize    Skip TCP optimization
#   -ExtensionId <id>       Skip auto-detect, use this ID
#   -Uninstall              Remove everything
```

**Verify installation:**

```powershell
# Check registry entries exist (installer uses HKCU, not HKLM)
Get-ItemProperty "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.display.calibration"
# Should show (Default) pointing to your native_host_manifest.json

Get-ItemProperty "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.canvas.accessibility"
# Should show (Default) pointing to your map_manifest.json
```

### Step 4: OpenClaw Gateway Setup (WSL)

If you're running OpenClaw as your AI backend, set up a dedicated instance for KZB.
This keeps it isolated from any existing OpenClaw installation.

**Clone and install (Linux filesystem for performance):**

```bash
# In WSL terminal
gh repo clone openclaw/openclaw ~/kzb-openclaw
cd ~/kzb-openclaw
pnpm install
```

**Start the gateway on port 18791:**

```bash
node openclaw.mjs --profile kzb gateway --bind loopback --port 18791
```

> The `--profile kzb` flag isolates all config under `~/.openclaw-kzb/`
> so it won't conflict with an existing OpenClaw on port 18789.

**Connect the browser (first time only):**

```bash
# Get the tokenized dashboard URL
node openclaw.mjs --profile kzb dashboard --no-open
# Open the printed URL in Chrome to sync the token
```

**Approve device pairing (first time only):**

```bash
# After opening the dashboard URL, approve the browser pairing:
node openclaw.mjs --profile kzb devices list
node openclaw.mjs --profile kzb devices approve <requestId>
```

**Copy API auth from existing installation (if you already have OpenClaw configured):**

```bash
cp ~/.openclaw/agents/main/agent/auth-profiles.json \
   ~/.openclaw-kzb/agents/dev/agent/auth-profiles.json
```

**Troubleshooting token mismatch:**
- Always start the gateway with `--profile kzb` (not `OPENCLAW_CONFIG_HOME`)
- If the browser shows "token mismatch", re-open the `dashboard --no-open` URL
- If the CLI shows "token mismatch", check that `gateway.remote.token` matches
  `gateway.auth.token` in `~/.openclaw-kzb/openclaw.json`

---

### Step 5: Configure Your Character

Edit the config for your build:

```powershell
# Open config in Notepad
notepad C:\ProgramData\DisplayCalibration\config.yaml

# Or copy a pre-made config:
copy C:\Users\YourName\Downloads\KZB\botter\configs\sorceress_blizzard.yaml `
     C:\ProgramData\DisplayCalibration\config.yaml
```

**Edit these sections:**

```yaml
# 1. Your class and build
character_class: Sorceress
build: Blizzard

# 2. Combat hotkeys (match your in-game keybinds!)
combat:
  attack_slots:
    preattack: 'e'       # Your first attack hotkey
    boss_primary: 'f'    # Main boss skill
    mob_primary: 'h'     # Normal mob skill

  primary_skill_key: 'f'       # Fallback
  mobility_skill_key: 'a'      # Teleport, Charge, etc.

# 3. Survival thresholds
survival:
  chicken_hp_pct: 30           # Exit at 30% HP
  hp_potion_pct: 75            # Drink at 75%

# 4. Town settings
town:
  task_order:
    - heal
    - stash
    - buy_potions

# 5. Farming sequence (which areas to farm)
farming:
  sequence:
    - name: Mephisto
      enabled: true
    - name: Diablo
      enabled: true
  max_game_time_mins: 30

# Full list of settings in README.md > Configuration Guide
```

### Step 6: Launch the Bot

```
1. Start the game
2. Create or load a single-player game (any act, any difficulty)
3. Character should be in town
4. Click the extension icon (top-right, puzzle piece)
5. Status should show "Agent: Connected" (green dot)
6. Watch the stats update in real-time
7. Bot auto-starts farming!
```

---

## Manual Installation (Advanced)

If you prefer to set things up manually or the script fails:

### Build Binaries

```powershell
cd C:\Users\YourName\Downloads\KZB\botter
cargo build --release
copy target\release\kzb_vision_agent.exe "C:\ProgramData\DisplayCalibration\chrome_helper.exe"

cd ..\maphack
cargo build --release
copy target\release\chrome_map_helper.exe "C:\ProgramData\Google\Chrome\NativeMessagingHosts\"
```

### Create Registry Entries

**For Vision Agent**, in PowerShell:

```powershell
$manifestPath = "$env:USERPROFILE\KZB\native-hosts\native_host_manifest.json"
$regPath = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.display.calibration"
New-Item -Path $regPath -Force -ErrorAction SilentlyContinue | Out-Null
Set-ItemProperty -Path $regPath -Name "(Default)" -Value $manifestPath
```

**For Map Helper:**

```powershell
$manifestPath = "$env:USERPROFILE\KZB\native-hosts\map_manifest.json"
$regPath = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.canvas.accessibility"
New-Item -Path $regPath -Force -ErrorAction SilentlyContinue | Out-Null
Set-ItemProperty -Path $regPath -Name "(Default)" -Value $manifestPath
```

### Create Native Host Manifests

**File: `%USERPROFILE%\KZB\native-hosts\native_host_manifest.json`**

```json
{
  "name": "com.chromium.display.calibration",
  "description": "Chrome Native Messaging Host",
  "path": "C:\\ProgramData\\DisplayCalibration\\chrome_helper.exe",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://YOUR_EXTENSION_ID/"]
}
```

**File: `%USERPROFILE%\KZB\native-hosts\map_manifest.json`**

```json
{
  "name": "com.chromium.canvas.accessibility",
  "description": "Chrome Native Messaging Host",
  "path": "C:\\ProgramData\\Google\\Chrome\\NativeMessagingHosts\\chrome_map_helper.exe",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://YOUR_EXTENSION_ID/"]
}
```

(Replace `YOUR_EXTENSION_ID` with the actual ID from chrome://extensions)

### Copy Config Directory

```powershell
mkdir "C:\ProgramData\DisplayCalibration\configs" -ErrorAction SilentlyContinue
copy "C:\Users\YourName\Downloads\KZB\botter\configs\*.yaml" `
      "C:\ProgramData\DisplayCalibration\configs\"
```

---

## Uninstall

### Option 1: Automated

```powershell
cd C:\Users\YourName\Downloads\KZB
.\install.ps1 -Uninstall
# Removes: registry entries, binaries, TCP optimizations
# Keeps: configs (manual cleanup) and Chrome extension (manual removal)
```

### Option 2: Manual

```powershell
# Remove registry entries
Remove-Item -Path "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.display.calibration" -Force
Remove-Item -Path "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.canvas.accessibility" -Force

# Remove binaries
Remove-Item "C:\ProgramData\DisplayCalibration\chrome_helper.exe" -Force
Remove-Item "C:\ProgramData\Google\Chrome\NativeMessagingHosts\chrome_map_helper.exe" -Force

# Remove TCP optimizations (Leatrix)
Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces" |
  ForEach-Object {
    Remove-ItemProperty $_.PSPath -Name TcpNoDelay -ErrorAction SilentlyContinue
    Remove-ItemProperty $_.PSPath -Name TcpAckFrequency -ErrorAction SilentlyContinue
  }

# Remove configs (optional, keeps your edits)
# rmdir "C:\ProgramData\DisplayCalibration\" -Recurse -Force

# Remove extension from Chrome
# chrome://extensions → Remove "Display Calibration Helper"
```

---

## Troubleshooting

### "Agent: Disconnected" in Extension Popup

**Cause**: Native host not installed or path wrong.

**Fix**:
1. Check registry exists:
   ```powershell
   Get-Item "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.display.calibration"
   ```
   If not found, re-run installer.

2. Check manifest path is correct:
   ```powershell
   Get-Content "$env:USERPROFILE\KZB\native-hosts\native_host_manifest.json"
   ```

3. Check binary exists:
   ```powershell
   Test-Path "C:\ProgramData\DisplayCalibration\chrome_helper.exe"
   ```

4. Check Chrome console for errors:
   - chrome://extensions
   - Click "Errors" under the extension

### "Could not load Chrome/Edge native host"

**Cause**: Extension ID mismatch or host not registered.

**Fix**:
1. Verify extension ID is correct:
   - chrome://extensions
   - Copy the blue ID string

2. Update manifest.json with correct ID:
   ```json
   "allowed_origins": ["chrome-extension://YOUR_ACTUAL_ID_HERE/"]
   ```

3. Reload extension (chrome://extensions → reload button)

### Bot doesn't attack / stands still

**Cause**: Attack hotkeys don't match in-game bindings.

**Fix**:
1. In-game, check your skill hotkeys (default is typically F1-F4 for attack skills)
2. Edit `C:\ProgramData\DisplayCalibration\config.yaml`:
   ```yaml
   combat:
     attack_slots:
       boss_primary: 'f'    # If F1 is your main attack, use 'F1' or just 'f'
   ```
3. Save and wait for bot to pick up config changes (live reload via popup)

### "Access is denied" when running installer

**Cause**: PowerShell doesn't have admin rights.

**Fix**:
1. Right-click PowerShell
2. Select "Run as Administrator"
3. Re-run installer

### Extension loads but map doesn't show

**Cause**: Map helper not installed or not connecting.

**Fix**:
1. Check map helper registry:
   ```powershell
   Get-Item "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.canvas.accessibility"
   ```

2. Check popup status: should show "Map Host: Connected"

3. Test map toggle: Ctrl+Shift+M in game window (should appear/disappear)

### TCP optimization didn't apply

**Cause**: Needs admin privileges.

**Fix**:
1. Right-click PowerShell → "Run as Administrator"
2. Re-run: `.\install.ps1`
3. To skip: `.\install.ps1 -SkipNetworkOptimize`
4. To verify, check registry:
   ```powershell
   Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\*" |
     Select-Object TcpNoDelay, TcpAckFrequency
   ```

### Game runs but bot never starts

**Cause**: Bot doesn't detect game loaded.

**Fix**:
1. Verify config loaded: Check Extension popup > Status section
2. Check character is in town (not in menu)
3. Wait 10 seconds (bot initializes on first frame capture)
4. Check log file:
   ```powershell
   notepad "C:\ProgramData\DisplayCalibration\agent.log"
   ```

---

## Verify Installation

Quick checklist:

- [ ] Native hosts registered in registry
- [ ] Binaries exist in Program Files
- [ ] Configs exist in ProgramData
- [ ] TCP optimization applied (TcpNoDelay=1, TcpAckFrequency=1)
- [ ] Extension loads in Chrome
- [ ] Extension popup shows "Agent: Connected"
- [ ] Game loads and bot stats update in popup
- [ ] Ctrl+Shift+M toggles map (if using maphack)

If all ✓, you're good to go!

---

## Next Steps

1. **Read CONFIG GUIDE** in README.md for full option reference
2. **Adjust humanization** settings (reaction time, idle pauses) to your liking
3. **Test in safe area** (Cold Plains, Andariel) before major farming runs
4. **Monitor logs** at `C:\ProgramData\DisplayCalibration\` for bot behavior
5. **Join community** (if applicable) for shared configs and tips

---

## Support

If you hit issues:

1. Check the troubleshooting section above
2. Look in the log files (`C:\ProgramData\DisplayCalibration\`)
3. Verify all prerequisites are met
4. Try a clean reinstall (uninstall, then re-run installer)

---

## Technical Details

### Native Messaging Protocol

```
Chrome Extension
    ↓ (stdio)
Native Host Process
    ↓ (4-byte LE len + JSON)
JSON Messages
    ↓
Agent Logic
    ↓
Return JSON (stats, acks, etc.)
```

### File Locations

```
Binaries:
  C:\ProgramData\DisplayCalibration\chrome_helper.exe        (vision agent)
  C:\ProgramData\Google\Chrome\NativeMessagingHosts\chrome_map_helper.exe  (map helper)

Manifests:
  %USERPROFILE%\KZB\native-hosts\native_host_manifest.json
  %USERPROFILE%\KZB\native-hosts\map_manifest.json

Configs:
  C:\ProgramData\DisplayCalibration\configs\

Logs:
  C:\ProgramData\DisplayCalibration\agent.log
  C:\ProgramData\DisplayCalibration\decision.log

Registry (HKCU):
  HKCU:\Software\Google\Chrome\NativeMessagingHosts\
    com.chromium.display.calibration
    com.chromium.canvas.accessibility
```

### Data Directory Resolution

Bot searches for config in this order:
1. Command-line argument: `d2_vision_agent.exe my_config.yaml`
2. Environment variable: `$env:D2R_CONFIG = "C:\path\to\config.yaml"`
3. Default: `C:\ProgramData\DisplayCalibration\config.yaml`

---

## Done!

KZB is ready. Load a game and watch it farm! 🤖

For detailed configuration options, see README.md > Configuration Guide.
