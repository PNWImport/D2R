# D2 Vision Agent v1.0.0

Production-ready Diablo II vision agent. Chrome native messaging disguise. Click install, go.

## Install (Windows)

```powershell
# 1. Build
cargo build --release

# 2. Install (as Admin)
.\deploy\install_host.ps1

# 3. Load extension
#    Chrome → chrome://extensions → Developer mode → Load unpacked → chrome_extension/
#    Copy the extension ID

# 4. Re-install with your extension ID
.\deploy\install_host.ps1 -ExtensionId "abcdefghijklmnopqrstuvwxyz1234"

# 5. (Optional) Customize config
copy deploy\default_config.yaml C:\ProgramData\DisplayCalibration\config.yaml
# Edit config.yaml — change class, thresholds, schedule

# 6. Done. Agent starts automatically when Chrome loads the extension.
```

## What Happens On Launch

1. Chrome spawns `chrome_helper.exe` as child process (legitimate process tree)
2. Agent loads `config.yaml` from `C:\ProgramData\DisplayCalibration\`
3. DXGI Desktop Duplication captures screen at 25 Hz → 16-shard lock-free buffer
4. Decision engine reads frames, applies kolbot-distilled thresholds
5. Thread-rotated SendInput pool dispatches with bezier mouse + gaussian timing
6. Chrome extension shows no UI, no notifications, no storage — zero traces

## Architecture

```
Chrome Extension (silent)
    ↕ stdio (4-byte LE + JSON)
chrome_helper.exe
    ├── Capture Thread     → DXGI → ShardedFrameBuffer (16 shards, 47M fps)
    ├── Decision Thread    → FrameState → Action (kolbot thresholds)
    ├── Input Pool (4 thr) → SendInput with rotation + jitter
    └── Stealth Stack      → Cadence control, handle lifecycle, decoy injection
```

## Config Quick Reference

| Setting | Default | What It Does |
|---|---|---|
| `chicken_hp_pct` | 30 | Exit game below this HP |
| `hp_potion_pct` | 75 | Drink HP pot below this |
| `reaction_mean_ms` | 280 | Average humanized delay |
| `survival_max_delay_ms` | 150 | Hard cap on survival reaction |
| `max_daily_hours` | 8 | Session limit per day |
| `primary_skill_key` | f | Main attack hotkey |
| `kite_threshold` | 6 | Enemies before kiting |

See `deploy/default_config.yaml` for all options.

## Chrome Extension Controls

The extension communicates via native messaging. Send JSON commands:

- `{"cmd": "get_stats"}` → frames, decisions, kills, potions, loots
- `{"cmd": "pause", "reason": "..."}` → pause agent
- `{"cmd": "resume"}` → resume
- `{"cmd": "update_config", "data": {...}}` → live config update
- `{"cmd": "shutdown"}` → clean exit

## Files

| File | Lines | Purpose |
|---|---|---|
| `src/main.rs` | 649 | Production binary + 12 unit tests |
| `src/vision/shard_buffer.rs` | 621 | 16-shard lock-free frame buffer |
| `src/vision/capture.rs` | 596 | DXGI screen capture + vision pipeline |
| `src/decision/engine.rs` | 746 | Priority-based decision engine |
| `src/stealth/*.rs` | 2136 | Full stealth stack (5 modules) |
| `src/native_messaging/mod.rs` | 580 | Chrome stdio protocol |
| `src/input/simulator.rs` | 230 | Bezier mouse + SendInput |
| `build.rs` | 16 | PE metadata stamping |
| `chrome_extension/` | 144 | Manifest v3 + background.js |
| `deploy/` | 135 | Installer + config + manifest |

**Total: 26 files, ~7,100 lines. 78 tests passing.**

## Stealth Properties

- **Process tree**: Chrome child process (legitimate — no PEB manipulation needed)
- **Binary name**: `chrome_helper.exe` with Google LLC PE metadata
- **Extension**: "Display Calibration Helper" — generic name, zero UI
- **Registry**: `com.chromium.display.calibration` under Chrome's NativeMessagingHosts
- **Capture**: Jittered burst mode, skip injection, long pause injection
- **Input**: 4-thread rotated SendInput with per-category cadence jitter
- **Handles**: Burst acquire/release lifecycle with forced timeout
- **Syscalls**: ETW jitter + decoy NtQuerySystemInformation injection
- **Logs**: `C:\ProgramData\DisplayCalibration\logs\` (no stdout)
