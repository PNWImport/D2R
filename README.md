# D2R Suite

One program. Three components, one Chrome extension.

```
extension/chrome_extension/   ← Load this in Chrome (single extension, both hosts)
botter/                        ← Vision agent source (Rust) — chrome_helper.exe
maphack/                       ← Map helper source (Rust)  — chrome_map_helper.exe
kolbot/                        ← D2 classic bot (D2BS/JS) — slimmed, production-ready
```

---

## Build

Requires Rust toolchain (`rustup`). Build both binaries:

```powershell
# Vision agent
cd botter
cargo build --release
# Output: target\release\d2_vision_agent.exe

# Map helper
cd ..\maphack
cargo build --release
# Output: target\release\d2r_map_helper.exe
```

---

## Install

Run each installer as Administrator with your Chrome extension ID:

```powershell
# Vision agent
cd botter
.\deploy\install_host.ps1 -ExtensionId "your-extension-id-here"

# Map helper
cd ..\maphack
.\installer\install_map_host.ps1 -ExtensionId "your-extension-id-here"
```

---

## Chrome Extension

1. Open `chrome://extensions`
2. Enable Developer mode
3. Load unpacked → select `extension/chrome_extension/`
4. Copy the extension ID
5. Re-run both installers with the ID

The extension connects to both native hosts automatically on startup.

**Keyboard shortcuts (map overlay):**
- `Ctrl+Shift+M` — toggle map
- `Ctrl+Shift+Up/Down` — opacity

---

## Character Configs

Pick the YAML for your build and copy it to the vision agent data directory:

```
botter/configs/
  sorceress_blizzard.yaml
  sorceress_meteorb.yaml
  paladin_hammerdin.yaml
  necromancer_fishymancer.yaml
  amazon_javazon.yaml
```

```powershell
copy botter\configs\sorceress_blizzard.yaml C:\ProgramData\DisplayCalibration\config.yaml
```

Edit `config.yaml` to match your in-game hotkey bindings before running.

---

## Kolbot (D2 Classic)

Located in `kolbot/`. Requires D2BS.

```
kolbot/
  D2Bot.exe            ← Manager
  d2bs/D2BS.dll        ← Core
  d2bs/kolbot/         ← Script library
  +setup/setup.ps1     ← Run first
```

1. Run `setup.bat` (or `+setup/setup.ps1`) to copy config files to their locations
2. Configure `+setup/starter/StarterConfig.js` and your class config in `d2bs/kolbot/libs/config/`
3. Launch D2Bot.exe

---

## Uninstall Map Helper

```powershell
cd maphack
.\installer\install_map_host.ps1 -Uninstall
```
