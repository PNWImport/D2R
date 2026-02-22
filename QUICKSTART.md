# KZB — Quick Start

Get farming in 5 minutes.

---

## 1. Build (2 min)

```powershell
cd C:\path\to\KZB
.\install.ps1
# Builds Rust binaries, installs native hosts, copies configs
# When prompted, load the extension in Chrome first (see step 2)
```

---

## 2. Load Extension (1 min)

```
1. Go to chrome://extensions
2. Enable "Developer mode" (top-right)
3. Click "Load unpacked"
4. Select: C:\path\to\KZB\extension\chrome_extension\
5. Copy the Extension ID (blue text under extension name)
6. Return to PowerShell installer, paste the ID when prompted
```

---

## 3. Configure (1 min)

```powershell
# Edit your character config
notepad C:\ProgramData\DisplayCalibration\config.yaml

# Key settings to change:
#   - character_class: Sorceress / Paladin / Amazon / etc.
#   - build: blizzard / hammerdin / javazon / etc.
#   - combat.attack_slots: map your hotkeys (F1-F12, a-z, etc.)
#   - survival.chicken_hp_pct: 30 (exit when HP < 30%)
```

---

## 4. Run (1 min)

```
1. Start D2R
2. Load or create a single-player game
3. Go to town (any act)
4. Click extension icon (puzzle piece, top-right)
5. Wait for "Agent: Connected" (green dot)
6. Bot auto-starts farming!
```

---

## Basic Hotkey Setup

For **Sorceress Blizzard**:
```yaml
combat:
  attack_slots:
    preattack: 'e'           # Pre-cast (Precast Skill)
    boss_primary: 'f'        # Blizzard (main attack)
    mob_primary: 'h'         # Blizzard (normal mobs)
    immune_primary: 'r'      # Fireball or other (physical immune)
```

For **Paladin Hammerdin**:
```yaml
combat:
  attack_slots:
    preattack: 'e'           # Fanaticism / Might
    boss_primary: 'f'        # Blessed Hammer (main)
    mob_primary: 'h'         # Blessed Hammer (mobs)
    immune_primary: 'r'      # Smite or Zeal (melee fallback)
```

For **Amazon Javazon**:
```yaml
combat:
  attack_slots:
    preattack: 'e'           # Inner Sight / Slow Missiles
    boss_primary: 'f'        # Lightning Fury (main)
    mob_primary: 'h'         # Lightning Fury (mobs)
    immune_primary: 'r'      # Guided Arrow or Strafe
```

**Get your actual hotkeys:**
1. In D2R, go to Options → Keybindings
2. Note which keys you've bound each skill to
3. Use those same keys in the config

---

## Adjust Bot Behavior

### Be More Aggressive
```yaml
humanization:
  reaction_mean_ms: 150           # React faster (default 280)
  potion_threshold_variance: 20   # More variance = less predictable
  idle_pause_rate: 0.01           # Less idle time

survival:
  chicken_hp_pct: 20              # Exit later (default 30)
  hp_potion_pct: 60               # Drink later
```

### Be More Cautious
```yaml
humanization:
  reaction_mean_ms: 400           # React slower
  aim_variance_px: 30             # More aim misses
  idle_pause_rate: 0.05           # More idle time

survival:
  chicken_hp_pct: 50              # Exit earlier
  hp_potion_pct: 85               # Drink sooner
  tp_retreat_pct: 50              # TP at 50%
```

### Humanize More
```yaml
humanization:
  potion_forget_rate: 0.15        # 15% chance to "forget" potions
  skill_miss_rate: 0.10           # 10% wrong key presses
  path_deviate_rate: 0.15         # Random pathing
  aggression_drift_per_hour: 0.08 # Get more aggressive over time
```

---

## Monitor Bot Activity

**Extension Popup (click extension icon):**
- **Status**: Agent/Map connection (green = connected)
- **Stats**: Frames captured, decisions made, potions used, loots picked, chickens executed
- **Uptime**: How long session has been running

**Log File:**
```powershell
# Watch in real-time (PowerShell v3+)
Get-Content "C:\ProgramData\DisplayCalibration\agent.log" -Wait

# Or open in editor
notepad "C:\ProgramData\DisplayCalibration\agent.log"
```

---

## Common Adjustments

| Goal | Setting | Value |
|------|---------|-------|
| Exit sooner on low HP | `survival.chicken_hp_pct` | 20 |
| Survive longer | `survival.chicken_hp_pct` | 50 |
| Drink potions less | `survival.hp_potion_pct` | 90 |
| Drink potions more | `survival.hp_potion_pct` | 50 |
| Run longer per game | `farming.max_game_time_mins` | 60 |
| Run shorter per game | `farming.max_game_time_mins` | 15 |
| React faster | `humanization.reaction_mean_ms` | 150 |
| React slower | `humanization.reaction_mean_ms` | 400 |
| Less humanization | `potion_forget_rate`, `skill_miss_rate` | 0.0 |
| More humanization | `potion_forget_rate`, `skill_miss_rate` | 0.15 |

---

## Troubleshooting

### "Agent: Disconnected"
```powershell
# Check if native host is installed
Get-Item "HKLM:\Software\Google\Chrome\NativeMessagingHosts\com.chromium.display.calibration"

# If error, re-run installer:
.\install.ps1
```

### Bot doesn't attack
```powershell
# Check hotkeys match in-game bindings
# Open D2R options > Keybindings
# Make sure config.yaml uses the same keys

# Example:
# If F1 is your main skill in D2R, use:
combat:
  attack_slots:
    boss_primary: 'F1'    # Or just 'f' for F1
```

### Map overlay doesn't show
```powershell
# In-game, press Ctrl+Shift+M
# If map doesn't appear, extension not connected to map helper
# Check extension popup: Map Host should be green
```

### Config changes don't apply
```powershell
# Save the YAML file (Ctrl+S in Notepad)
# Close game and reload
# Bot checks config at game start
```

---

## Pre-Made Configs

**Included character configs** (in `botter/configs/`):

1. `sorceress_blizzard.yaml` — Blizzard/Orb, pure damage
2. `sorceress_light.yaml` — Lightning, chain damage
3. `paladin_hammerdin.yaml` — Blessed Hammer, fast clear
4. `amazon_javazon.yaml` — Lightning Fury, projectile
5. `necromancer_fishymancer.yaml` — Summon + Mage hybrid
6. `assassin_trapsin.yaml` — Lightning/Death traps
7. `barbarian_ww.yaml` — Whirlwind melee
8. `druid_wind.yaml` — Tornado/Cyclone wind skills

Copy any of these to `C:\ProgramData\DisplayCalibration\config.yaml` and edit to match your hotkeys.

---

## Next Steps

- **Full config guide**: See `README.md` > Configuration Guide
- **Architecture**: See `STRUCTURE.md` for codebase tour
- **Installation issues**: See `INSTALL.md` > Troubleshooting
- **Version history**: See `CHANGELOG.md` for features per version

---

## Need Help?

1. **Config guide**: `README.md` (full option reference)
2. **Install issues**: `INSTALL.md` (detailed troubleshooting)
3. **Code questions**: `STRUCTURE.md` (architecture & files)
4. **Version history**: `CHANGELOG.md` (what's new)

**Everything you need is documented.** Happy farming! 🤖

---

## Session Management

KZB respects human play patterns:

```yaml
session:
  max_daily_hours: 8.0              # Stop after 8 hours
  session_min_minutes: 60           # Run at least 60 min before break
  session_max_minutes: 180          # Max 180 min (3 hours) before break
  break_min_minutes: 30             # Break 30-120 min
  break_max_minutes: 120

  allowed_start_hour: 9             # Only run 9 AM - 11 PM
  allowed_end_hour: 23
  day_off: 2                        # Tuesday (0=Sun) off per week

  short_break_rate: 0.12            # ~5 min between breaks
  long_break_rate: 0.04             # ~25 min between breaks
```

The bot will pause and wait during breaks, and refuse to start outside allowed hours. Plan accordingly!

---

## Tips & Tricks

### Multi-Char Setup
```powershell
# Create separate configs per character
copy C:\ProgramData\DisplayCalibration\config.yaml sorceress.yaml
copy C:\ProgramData\DisplayCalibration\config.yaml paladin.yaml

# Edit each, then swap when needed:
copy sorceress.yaml C:\ProgramData\DisplayCalibration\config.yaml
# ... bot picks up new config on next game
```

### Farm Different Areas
Edit `farming.sequence` in config:
```yaml
farming:
  sequence:
    - name: Mephisto
      enabled: true
    - name: Andariel
      enabled: true
    - name: Countess
      enabled: false  # Skip
```

### Test New Config
```powershell
# Load a test game (Cold Plains, Andariel)
# Watch stats in popup
# Check log file for decision reasons
# If satisfied, run main farming
```

### Backup Your Config
```powershell
# Before major edits
copy C:\ProgramData\DisplayCalibration\config.yaml config.backup.yaml

# Restore if needed
copy config.backup.yaml C:\ProgramData\DisplayCalibration\config.yaml
```

---

That's it! You're farming. 🎯
