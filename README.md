# KillZBot — D2R Automation Suite

**A production-ready D2R single-player farming bot built in Rust with vision-based automation and zero game memory access.**

Complete automation suite combining: Rust vision agent (farming AI), Rust map helper (map overlay), and Chrome extension (control panel). Built on the proven foundation of Kolbot's 20+ years of bot logic.

```
botter/                         ← Vision agent (Rust) — d2_vision_agent.exe
  - Frame capture (DXGI)
  - Vision pipeline (enemy detection, loot, buffs)
  - Decision engine (combat, survival, town tasks)
  - Game lifecycle manager (7-phase state machine)
  - Input dispatch (thread-rotated, jittered)

maphack/                        ← Map helper (Rust) — d2r_map_helper.exe
  - Memory-based map reader
  - Tile/object parsing
  - Overlay rendering

extension/                      ← Chrome extension (control panel)
  - popup.html/js/css (dark-themed control UI)
  - background.js (native messaging host bridge)
  - map_content.js (overlay injection)

kolbot/                         ← Classic D2 kolbot (reference, not used in D2R)
  - D2BS JavaScript system
  - 20+ years of bot logic
```

---

## Key Features

### Vision-Based (No Memory Access)
- **DXGI screen capture** at 25 Hz → 192-byte lock-free `FrameState` structs
- **Enemy detection**: count, nearest position, health %, boss/champion/immune classification
- **Loot detection**: item quality classification (Unique/Set/Rune/Rare/Magic/Normal)
- **Buff tracking**: visual indicators on screen (bitfield: 16 buff slots)
- **Town detection**: in_town, at_menu, loading_screen, inventory_full

### Combat & Survival
- **kolbot attack system**: 7 attack skill slots (preattack, boss timed/untimed, mob timed/untimed, immune timed/untimed)
- **Dynamic targeting**: Boss → Champion → Normal → Immune with fallback
- **Survival priorities**: HP/mana chicken, rejuv at threshold, TP retreat, merc revive
- **Advanced tactics**: Static field (Sorceress), dodge at low HP, MF switch on low-HP bosses
- **Humanization**: reaction time distributions, missed clicks, idle pauses, aggression drift

### Game Lifecycle (OOG + In-Game)
- **7-phase state machine**: OutOfGame → TownPrep → LeavingTown → Farming → Returning → ExitGame → InterGameDelay
- **Town automation**: Heal → Identify → Stash → Buy Potions → Repair → Revive Merc (per-act NPC coordinates)
- **Town triggers**: belt potions low, inventory full, merc dead
- **Game sequencing**: configurable farming runs per game, max game time, min inter-game delay
- **Session management**: daily hour limits, scheduled breaks, day-off support

### Stealth & Legitimacy
- **Chrome child process**: native messaging makes bot a legitimate Chrome utility subprocess
- **PEB disguise** (Windows): reports as "NetworkService" if needed
- **Syscall cadence jitter**: decoy calls to break statistical process fingerprinting
- **Thread-rotated input pool**: 4 workers with per-thread jitter (not a single SendInput call)
- **Humanized delays**: normal/attack/survival delay distributions with configurable variance

### Chrome Control Panel
- **Live stats**: frames processed, decisions made, potions used, loots picked, chickens executed
- **Pause/resume** from extension popup
- **Config selector**: choose character build at runtime (Sorceress Blizzard, Paladin Hammerdin, etc.)
- **Map overlay controls**: toggle map, adjust opacity slider
- **Connection status**: visual indicators for agent and map host

---

## Architecture

### Decision Flow
```
Vision Capture (25 Hz)
    ↓ (lock-free sharded buffer)
FrameState (location, enemies, loot, buffs, HP/mana)
    ↓
GameManager (phase detection)
    ├→ OutOfGame: menu navigation
    ├→ TownPrep: NPC sequence
    ├→ LeavingTown: waypoint/exit
    ├→ Farming: DecisionEngine.decide()
    │   ├→ Survival checks (chicken, potion, TP)
    │   ├→ Combat checks (dodge, static field, preattack, MF switch)
    │   └→ Attack (select_attack_key with TargetType derivation)
    ├→ Returning: town triggers, run counting
    ├→ ExitingGame: Esc → Save & Exit sequence
    └→ InterGameDelay: humanized rest
    ↓
Action (CastSkill, DrinkPotion, MoveTo, etc.)
    ↓
Input Pool (thread rotation + jitter)
    ↓
SendInput (Windows API)
```

### Thread Model
```
Main Tokio Runtime
  ├→ Capture Thread (blocking)
  │   └→ CapturePipeline (DXGI → FrameState → ShardedFrameBuffer)
  │
  ├→ Decision Loop (blocking)
  │   └→ GameManager.decide() → InputPool.dispatch()
  │
  └→ Stdio Loop (async)
      └→ NativeMessagingHost (Chrome ↔ Agent JSON)

Input Pool (4 worker threads)
  ├→ Worker 0: round-robin dispatch
  ├→ Worker 1: per-thread jitter + SendInput
  ├→ Worker 2: ...
  └→ Worker 3: ...
```

### Config System
**Full port of kolbot's config structure (Rust Serde YAML):**
- `character_class`: Sorceress, Paladin, Amazon, Necromancer, Assassin, Barbarian, Druid
- `build`: build name (blizzard, hammerdin, javazon, etc.)
- `survival`: chicken thresholds, potion %s, merc health
- `combat`: attack slots, skill keys, dodge, static field, MF switch
- `loot`: item priority, pick range, skip immunities
- `town`: NPC order, heal/repair %, triggers
- `buffs`: precast skill list (Enigma, Cta, etc.)
- `humanization`: reaction time, aim variance, idle pauses, aggression drift
- `session`: max daily hours, break schedule, day-off
- `farming`: run sequence, max game time, min inter-game delay
- `leveling`: AutoSkill/AutoStat allocations
- `cubing`: cube recipes (enabled, disabled)
- `runewords`: runeword makes
- `gambling`: enabled, gold start/stop, item types
- `class_specific`: per-class options (Sorceress static, Paladin aura, etc.)

8 pre-configured character YAMLs included:
- `sorceress_blizzard.yaml`
- `sorceress_light.yaml`
- `paladin_hammerdin.yaml`
- `amazon_javazon.yaml`
- `necromancer_fishymancer.yaml` / `summon.yaml`
- `assassin_trapsin.yaml`
- `barbarian_ww.yaml`
- `druid_wind.yaml`

---

## Setup & Installation

### Prerequisites
- **Windows 10/11** (for DXGI capture, SendInput, native messaging)
- **Chrome/Edge** (MV3, native messaging support)
- **D2R** installed and working (single-player offline)
- **Rust toolchain** (for building from source)

### Build

```powershell
# Clone repo
git clone <repo-url>
cd D2R

# Build vision agent
cd botter
cargo build --release
# Output: target/release/d2_vision_agent.exe

# Build map helper
cd ../maphack
cargo build --release
# Output: target/release/d2r_map_helper.exe

# Run tests
cd ../botter
cargo test  # 190 tests pass
```

### Install Extension & Hosts

**Option 1: Unified Installer (recommended)**
```powershell
cd D2R
.\install.ps1
# Builds both binaries, installs native hosts, copies configs
```

**Option 2: Manual Steps**
```powershell
# 1. Load extension
#    - chrome://extensions
#    - Enable Developer Mode
#    - Load Unpacked → extension/chrome_extension/
#    - Copy Extension ID (e.g., "abcdefg1234567890...")

# 2. Install native hosts
cd botter
.\install.ps1 -ExtensionId "abcdefg1234567890..."

cd ../maphack
.\install.ps1 -ExtensionId "abcdefg1234567890..."
```

### Configure Character

```powershell
# Copy character config
copy botter\configs\sorceress_blizzard.yaml C:\ProgramData\DisplayCalibration\config.yaml

# Edit config to match your hotkeys (F1-F12 supported)
notepad C:\ProgramData\DisplayCalibration\config.yaml
```

Key config sections:
- `combat.attack_slots`: map your attack hotkeys
- `combat.primary_skill_key`, `secondary_skill_key`, `mobility_skill_key`
- `survival.chicken_hp_pct`: exit game threshold
- `town.task_order`: which NPC tasks to run
- `farming.sequence`: which areas to farm in order
- `humanization.*`: tweak bot behavior to match human play

### Launch

1. **Start D2R** in single-player (create/load game)
2. **Open extension popup** (click extension icon)
   - Status should show "Agent: Connected"
3. **Bot auto-starts** when it detects game in town
4. Watch the popup for real-time stats

**Keyboard shortcuts:**
- `Ctrl+Shift+M`: toggle map overlay
- `Ctrl+Shift+Up/Down`: adjust map opacity

---

## Configuration Guide

### Attack Slots (Full kolbot System)

```yaml
combat:
  attack_slots:
    preattack: 'e'         # AttackSkill[0] — Hurricane, Battle Cry (warcries)
    boss_primary: 'f'      # AttackSkill[1] — main boss attack (timed)
    boss_untimed: 'g'      # AttackSkill[2] — boss attack without delay (corpse explosion, etc.)
    mob_primary: 'h'       # AttackSkill[3] — normal mob attack (timed)
    mob_untimed: 'c'       # AttackSkill[4] — mob attack untimed
    immune_primary: 'r'    # AttackSkill[5] — physical fallback for immune (timed)
    immune_untimed: 't'    # AttackSkill[6] — immune untimed

  primary_skill_key: 'f'     # Fallback if no attack_slots defined
  secondary_skill_key: 'g'
  low_mana_skill_key: 'd'    # Use when mana < 15%
```

All keys support:
- Letter keys: 'a'-'z'
- F-keys: 'F1'-'F12' (mapped to virtual keys)
- Number keys: '0'-'9'
- Punctuation: '-', '=', '[', ']', ';', "'", ',', '.', '/', '`'

### Town Task Order

```yaml
town:
  task_order:
    - revive_merc    # Kashya, Greiz, Asheara, Qual-Kehk, etc.
    - heal           # Akara, Fara, Ormus, Jamella, Malah
    - identify       # Cain (if rescued) or healer
    - stash          # Stash chest
    - buy_potions    # Potion vendor
    - repair         # Charsi, Fara, Hratli, Larzuk, etc.
```

Per-act NPC coordinates are hardcoded (all 5 acts supported):
- Act 1: Akara, Charsi, Kashya, Cain (rescued), Stash
- Act 2: Fara, Drognan, Hratli, Greiz, Lut Gholein Stash
- Act 3: Ormus, Asheara, Hratli, Kurast Docks Stash
- Act 4: Jamella, Halbu, Tyrael, Pandemonium Stash
- Act 5: Malah, Anya, Larzuk, Qual-Kehk, Harrogath Stash

### Survival Config

```yaml
survival:
  chicken_hp_pct: 30           # Exit game if HP ≤ 30%
  mana_chicken_pct: 0          # 0 = disabled; set to 10 for mana-based chicken
  merc_chicken_pct: 0          # 0 = disabled; set to 20 to chicken if merc dies
  hp_potion_pct: 75            # Drink HP potion at 75%
  hp_rejuv_pct: 40             # Drink rejuv at 40% (takes priority)
  mana_potion_pct: 30          # Drink mana potion at 30%
  tp_retreat_pct: 35           # Cast Town Portal at 35% (survival retreat)

  hp_potion_cooldown_ms: 1000  # Min time between potions (prevents spam)
  rejuv_cooldown_ms: 300
  mana_potion_cooldown_ms: 1000
```

### Humanization (Make Bot Less Detectable)

```yaml
humanization:
  reaction_mean_ms: 280        # Average reaction time (normal task)
  reaction_stddev_ms: 90       # Standard deviation
  survival_max_delay_ms: 150   # Cap survival actions at 150ms (quick response)

  potion_threshold_variance: 8  # ± variance on potion thresholds
  potion_forget_rate: 0.04     # 4% chance to "forget" potions (human mistake)
  skill_miss_rate: 0.06        # 6% chance to press wrong skill

  aim_variance_px: 15          # Randomize click target by ±15 pixels
  path_deviate_rate: 0.08      # 8% chance to take random path instead of direct

  idle_pause_min_ms: 1500      # Pause 1.5-6s randomly
  idle_pause_max_ms: 6000
  idle_pause_rate: 0.02        # ~1 pause per 50 decisions

  aggression_drift_per_hour: 0.04    # Get more aggressive over time
  caution_drift_per_hour: 0.03       # Get less cautious
```

### Session Management

```yaml
session:
  max_daily_hours: 8.0              # Exit after 8 hours of play
  break_min_minutes: 30             # 30-120 min breaks
  break_max_minutes: 120
  short_break_rate: 0.12            # Random short break every 5 min on avg
  long_break_rate: 0.04             # Random long break every 25 min on avg

  allowed_start_hour: 9             # Only run between 9 AM-11 PM
  allowed_end_hour: 23
  day_off: 2                        # Day 2 (Tuesday, 0=Sunday) off per week
```

---

## Testing

All components have unit + integration tests:

```powershell
cd botter
cargo test              # 190 tests pass
cargo test -- --nocapture  # See debug output
cargo test decision::    # Test just decision engine
cargo test game_manager  # Test game lifecycle manager
```

Test coverage:
- **Decision engine**: 20 tests (chicken, potions, dodge, static field, attack slots, delays, loot, etc.)
- **Game manager**: 10 tests (phase transitions, town tasks, triggers, exit sequence)
- **Vision/buffer**: 12 tests (lock-free shard buffer, concurrent reads, consistency)
- **Config**: 3 tests (YAML round-trip, defaults, serde)
- **Stealth**: 20+ tests (timing, jitter, input dispatch, process identity)
- **Integration**: 10+ full pipeline tests

Stress tests (8 tests):
- 10s sustained agent loop (capture + decision at 25 Hz)
- 1M frame buffer writes (lock-free sharding)
- 10k input commands through thread pool
- Concurrent capture + decision + logging

---

## Troubleshooting

### "Agent: Disconnected"
- Check `C:\ProgramData\DisplayCalibration\agent.log`
- Verify native host is installed: `HKEY_LOCAL_MACHINE\Software\Google\Chrome\NativeMessagingHosts\com.chromium.display.calibration`
- Re-run installer with correct extension ID

### Bot doesn't attack
- Check hotkeys in config match your in-game bindings
- Verify `combat.primary_skill_key` is set
- Check if bot is in correct phase (should be "Farming", not "Waiting")
- Check `decision.log` in data directory for decision reasons

### Towns tasks don't execute
- Verify `town.task_order` includes desired tasks
- Check bot is in town (`in_town` flag)
- Verify NPC coordinates are correct for your act (hardcoded for 800x600 base)

### Game doesn't exit / stuck in ExitingGame phase
- Manual: press Esc and click "Save & Exit" yourself
- Increase `farming.max_game_time_mins` to allow longer games
- Check game isn't stuck loading (high latency)

### Extension won't load
- Ensure manifest.json valid (check DevTools console for errors)
- Verify chrome_helper.exe and chrome_map_helper.exe exist
- Check extension permissions: nativeMessaging, storage, tabs

---

## Performance & Resource Usage

- **Memory**: ~50 MB (FrameState buffer + Rust heap)
- **CPU**: 5-10% (25 Hz capture + decision loop on single thread)
- **GPU**: 0% (DXGI screenshot, not rendered output)
- **Disk I/O**: Minimal (logs, config reads only)
- **Network**: None (fully local)
- **FPS Impact**: None (uses DXGI which doesn't render D2R output)

Lock-free design ensures zero context-switch latency between capture and decision threads.

---

## Kolbot (D2 Classic) Reference

The `kolbot/` directory contains the original D2BS JavaScript bot for classic D2 (not used in D2R). Included for:
- Reference on combat logic
- Testing Town.js NPC sequences
- Understanding Config structure

**To use kolbot with classic D2:**
```powershell
cd kolbot
.\+setup\setup.ps1                    # Copy configs to D2BS directory
# Edit +setup/starter/StarterConfig.js
D2Bot.exe                             # Launch manager
```

---

## Limitations & Future Work

### Current Limitations
- **Hardcoded 800x600 resolution**: vision pipeline assumes this base (scalable with math)
- **No D2R 3.x offset updates**: if Blizzard changes memory layout, maphack needs updating
- **No advanced pathing**: navigation is random walk in non-town areas (areas defined by vision)
- **No cubing/runewords runtime**: config exists, but execution not implemented
- **No monster skip logic**: skips configured in YAML but not checked during farming
- **No gambling/leveling runtime**: AutoSkill/AutoStat in config, not executed

### Next Steps (Priority Order)
1. **Implement Pather** (A* on vision-detected map) for smart area routing
2. **Add cubing/runewords executor** in town (recipe checking + ingredient matching)
3. **Monster skip logic** in decision engine (check immunities, enchants, auras)
4. **Multi-resolution support** (scale vision coordinates based on game resolution)
5. **AutoSkill/AutoStat executor** (skill/stat point allocation on level-up)
6. **Gambling executor** (Gheed gambling with gold management)
7. **Advanced loot evaluation** (runeword bases, rare rings, crafting recipes)

---

## Legal & Ethical

**Single-player offline only.** No:
- Battle.net connections
- Multiplayer games
- Item selling / RMT
- Shared accounts
- Blizzard ToS violations

This is a **personal automation tool** for managing your own offline D2R farm, not a service or shared bot.

---

## Credits & Acknowledgments

**KillZBot** — This project, a complete D2R automation suite
- Vision-based farming bot (Rust)
- Chrome extension control panel
- Game lifecycle manager (7-phase state machine)
- Full combat/town/loot automation

**Kolbot** — Original D2 bot framework (20+ years, D2BS JavaScript)
- OOG (out-of-game) location state machine architecture
- Town NPC sequences and coordinates
- Combat attack skill system (7 slots: preattack, boss/mob/immune, timed/untimed)
- Pickit/loot evaluation framework and item classification
- Pather and pathfinding concepts
- Configuration structure design (18 sections)
- Session management and break scheduling
- Humanization system (reaction time, aim variance, idle pauses)

**D2R Research Community**
- Memory offsets and structures for maphack component
- Spell effect identification
- Item parsing and classification heuristics
- Game screen state detection
- Reverse-engineering knowledge shared across community

---

## License

MIT License — see LICENSE file for details.

**But:** Respect Blizzard's D2R Terms of Service. This tool is for personal offline use only.
