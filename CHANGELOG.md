# Changelog — KZB

Complete version history of KZB, a production D2R farming automation suite built in Rust.

---

## [1.5.0] — 2026-02-23

### Performance & Architecture Overhaul

#### QuadCache Four-Lane Acceleration (`quad_cache.rs`)
- **Lane 2 (Structural)**: Farm run scripts pre-indexed at startup for O(1) lookup
  - `PreparedRun` struct with act derivation and boss detection
  - `act_for_run()` / `is_boss_run()` helpers for all D2R areas
  - `run_sequence` resolved once at warm, reused every cycle
- **Lane 3 (Metric Range)**: Survival thresholds flattened to `ThresholdBins`
  - Replaces 11 `self.config.survival.*` traversals per tick with direct field reads
  - 12 flat fields (chicken_hp, rejuv_hp, hp_potion, cooldowns, etc.)
  - Re-materialized on every `reload_config` — zero hot-path overhead
- **Lane 4 (Hot Joins)**: Recurring `(HpBin × in_combat × has_loot)` pattern telemetry
  - `HotKey` lookup table with hit counters
  - `SpanFeatures` emitted to optional LLM wrapper (openclaw)
  - `top_patterns(n)` for session context and diagnostics
  - Hit rate tracking for cache effectiveness monitoring
- **Lane 1 (Exact Query)**: Deliberately unused — game states are never pixel-identical
- Total footprint: ~22 KB (runs + thresholds + hot keys), all in agent-private heap

#### Dual Tick Drain — Config Update Latency Fix
- Added second `cmd_rx.try_recv()` drain after action execution in `main.rs`
- Config updates (chicken_hp, hp_potion_pct, etc.) now processed twice per tick:
  1. At tick start (existing)
  2. After action dispatch, before idle sleep (new)
- Worst-case "missed chicken/potion" window: **40ms → ~5ms** (5.2× speedup)
- Mean config update latency: **23.5ms → 4.5ms** (Monte Carlo verified, 50K runs)

#### Direct JSON Deserialization
- Eliminated `serde_json::to_string()` → `serde_yaml::from_str()` roundabout
- Now uses `serde_json::from_value::<AgentConfig>(data)` directly
- Saves ~2ms per config update and removes unnecessary serde_yaml dependency on hot path

#### Progression Engine & Script Executor
- `progression.rs`: Quest state tracking, difficulty progression, visual cue detection
  - Script sequence with area/quest/boss steps
  - `should_run()` for conditional script execution
- `script_executor.rs`: Script step execution with visual cue verification

#### Leatrix-Style TCP Optimization (Installer)
- Added `TcpNoDelay=1` and `TcpAckFrequency=1` registry tweaks to `install.ps1`
- Applied to all active network interfaces at install time
- Reduces TCP latency for D2R online play (~15-40ms improvement)
- New `-SkipNetworkOptimize` flag to opt out
- Admin privilege check with graceful skip
- Uninstall path reverts registry changes

#### Code Quality
- Full `cargo clippy` cleanup: **41 warnings → 0 warnings**
  - Fixed dead code annotations, vec init patterns, identical if blocks
  - Replaced manual Default impls with `#[derive(Default)]`
  - Fixed doc formatting, range contains, iterator patterns
  - Applied `is_multiple_of()`, `.clamp()`, `is_some_and()` idioms
- `cargo fmt` applied across all 23 touched files
- Removed dead `HostResponse` struct from native messaging (unused — responses are raw `json!()`)
- All 8 stress tests passing

#### Latency Profiler
- New `latency_profile.py` Monte Carlo simulator (50,000 runs per scenario)
- Three scenarios: standard (5KB), survival-critical (500B), full reload (50KB)
- V1 vs V2 comparison with statistical analysis
- Results documented in `LATENCY_ANALYSIS.md`

---

## [1.4.0] — 2026-02-23

### Chrome Extension GUI Rebuild
- Rebuilt popup.html to full 11-tab GUI with all 503 kolbot settings and 77 scripts
- popup.html: 1,521 LOC with Act-based script organization, sub-options, inventory grid
- popup.js: 372 LOC with chrome API guard, debounced save, sub-options toggle
- popup.css: 615 LOC with dark D2R theme, responsive tab bar, class sections
- background.js: 375 LOC with dual native host management, stats caching
- map_content.js: 260 LOC with overlay injection, keyboard shortcuts
- Added test_gui.html test harness (blob URL iframe, chrome mock, 19/19 automated tests)
- Added test_serve.sh helper for local HTTP serving
- Fixed 8 bugs: load order, null deref, duplicate data-cfg keys, service worker cleanup
- Extension version bumped to 1.4.0

---

## [1.0.0] — 2026-02-22

### KZB v1.0: Complete D2R Farming Automation Suite (Rust + Chrome)

#### Rust Vision Agent (`botter/`)
- **Frame capture** (DXGI Desktop Duplication)
  - 25 Hz continuous capture with skip/burst mode
  - Lock-free sharded buffer (16 shards, ~192 bytes, zero contention)
  - Concurrent producer/consumer with ABA protection

- **Vision pipeline** (pixel-based, no memory access)
  - Enemy detection: count, nearest position, health bar reading
  - Boss/champion/normal/immune classification (health bar width heuristic)
  - Loot detection: item quality classification, text hash dedup
  - Buff tracking: 16 bitfield for active buff indicators
  - Merc HP reading, belt potion detection, inventory status

- **Decision engine** (full kolbot port)
  - 7 attack skill slots (preattack, boss/mob/immune, timed/untimed)
  - Priority-based decision system (survival → combat → loot)
  - Survival: HP/mana/merc chicken, rejuv, potions, TP retreat
  - Combat: dodge, static field (Sorceress), preattack, MF switch
  - Humanization: reaction time distributions, missed clicks, idle pauses
  - Loot: rune/unique priority, distance-based sorting
  - Buff recasting with visual detection

- **Game lifecycle manager** (OOG + in-game automation)
  - 7-phase state machine (OutOfGame → TownPrep → LeavingTown → Farming → Returning → ExitGame → InterGameDelay)
  - Town automation: Heal → Identify → Stash → BuyPotions → Repair → ReviveMerc
  - Per-act NPC coordinates (Act 1-5)
  - Town triggers: belt low, inventory full, merc dead
  - Game exit sequence, inter-game delays, run counting
  - Session management: daily hour limits, breaks, day-off support

- **Input dispatch** (stealth & legitimacy)
  - Thread-rotated input pool (4 workers, round-robin)
  - Per-thread jitter on SendInput calls
  - Humanized delays (normal/attack/survival distributions)
  - Support for F1-F12 keys, punctuation

- **Stealth features**
  - Chrome child process (native messaging = legitimate subprocess)
  - PEB disguise (Windows, reports as NetworkService)
  - Syscall cadence jitter (decoy calls for fingerprint breaking)
  - Handle table fencing (randomized pseudo-handles)
  - Process identity spoofing (command line masking)

- **Configuration** (full Serde YAML)
  - 8 pre-configured character YAMLs (Sorceress, Paladin, Amazon, Necromancer, Assassin, Barbarian, Druid)
  - 18 config sections: survival, combat, loot, town, buffs, humanization, session, farming, leveling, cubing, runewords, gambling, class_specific, monster_skip, clear, merc, inventory
  - Hot-reload via Chrome popup
  - serde(default) on all optional fields for YAML backward-compatibility

- **Testing**
  - 85 library unit tests
  - 20 decision engine tests (chicken, potions, dodge, static field, attack slots, delays, loot)
  - 10 game manager tests (phase transitions, town tasks, triggers, exit sequence)
  - 99 binary integration tests (full pipeline, config round-trip, concurrent stats)
  - 8 stress tests (10s sustained loop, 1M frame writes, 10k input commands)
  - **Total: 192/192 passing**

#### Rust Map Helper (`maphack/`)
- Memory-based D2R map reader
- Tile and object parsing
- Overlay rendering to Chrome content script
- Map caching with statistics

#### Chrome Extension (`extension/`)
- **Manifest v3** (native messaging support)
- **Popup control panel** (dark D2R-themed UI)
  - Status indicators (Agent/Map host connection)
  - Pause/Resume buttons
  - Live stats: frames, decisions, potions, loots, chickens, uptime
  - Config selector (character build picker)
  - Map overlay controls (toggle, opacity slider)
  - 2s stat polling interval
  - Persistent opacity/map toggle via chrome.storage

- **Background service worker** (native host bridge)
  - Two native messaging hosts:
    - `com.chromium.display.calibration` (vision agent)
    - `com.d2vision.map` (map helper)
  - Stats caching (lastAgentStats for instant popup display)
  - Request timeouts (3s for async native calls)
  - Commands: pause, resume, update_config, getStatus

- **Content script** (map overlay injection)
  - Map rendering on all websites (overlay only on D2R window)
  - Opacity control
  - Keyboard shortcuts: Ctrl+Shift+M (toggle), Ctrl+Shift+Up/Down (opacity)

- **Keyboard shortcuts** (game-wide)
  - Ctrl+Shift+M: toggle map overlay
  - Ctrl+Shift+Up: increase map opacity
  - Ctrl+Shift+Down: decrease map opacity

#### Installers & Build System
- **Unified install script** (`install.ps1`)
  - Builds both Rust binaries from source
  - Registers native messaging hosts in Chrome registry
  - Copies config files to data directory
  - Supports `-Uninstall`, `-SkipBuild`, `-ExtensionOnly` modes
  - Batch wrapper for non-PowerShell compatibility

- **Native host registration**
  - Automated registry entries for Windows
  - Per-host JSON manifest (protocol, path, allowed extensions)

#### Documentation
- Comprehensive README.md with architecture, setup, and config guide
- This CHANGELOG tracking all changes
- Test documentation in source code comments

#### Configuration Files
8 character YAMLs pre-configured:
- `sorceress_blizzard.yaml` — Blizzard/Orb Sorceress
- `sorceress_meteorb.yaml` — Meteorb Sorceress
- `paladin_hammerdin.yaml` — Hammerdin Paladin
- `amazon_javazon.yaml` — Javazon Amazon
- `necromancer_fishymancer.yaml` — Fishymancer Necromancer (summon/mage hybrid)
- `assassin_trapsin.yaml` — Trapsin Assassin
- `barbarian_whirlwind.yaml` — Whirlwind Barbarian
- `druid_wind.yaml` — Wind Druid

---

## Architecture Milestones (Dev History)

### Phase 1: Core Vision (Commits 1-5)
- DXGI frame capture
- Lock-free sharded buffer
- Basic FrameState struct
- Enemy detection (count, nearest position)
- Boss/champion heuristics

### Phase 2: Decision Engine (Commits 6-10)
- DecisionEngine core
- Priority-based decision system
- Attack skill system (7 slots)
- Survival checks (chicken, potions, TP)
- Humanization (delays, variance)

### Phase 3: Kolbot Port (Commits 11-15)
- Full config structure (18 sections, 100+ fields)
- AgentConfig with serde YAML
- 8 character YAML configs
- Combat system (dodge, static field, MF switch)
- Buff recasting

### Phase 4: Game Lifecycle (Commits 16-20)
- GameManager (7-phase state machine)
- Town automation (NPC sequences)
- Per-act NPC coordinates
- Game exit/inter-game delays
- Run counting and session management

### Phase 5: Stealth & Legitimacy (Commits 21-25)
- Chrome native messaging setup
- PEB disguise (Windows)
- Syscall cadence jitter
- Handle table fencing
- Thread-rotated input pool

### Phase 6: Chrome Extension (Commits 26-30)
- Manifest v3 MV3 setup
- popup.html/js/css control panel
- background.js native host bridge
- map_content.js overlay injection
- Keyboard shortcuts

### Phase 7: Polish & Release (Commits 31-35)
- Unified installer (install.ps1)
- Config file selector (resolve_config_path)
- YAML serde defaults (backward-compat)
- Vision pipeline expansion (merc HP, belt, immune detection)
- F-key + punctuation VK codes
- 192 tests (85 lib + 99 bin + 8 stress)
- Documentation (README, CHANGELOG, config guide)

---

## Known Limitations

### Resolution Hardcoding
- NPC coordinates and UI detection assume 800x600 base resolution
- Scalable with math but not yet implemented
- Future: dynamic resolution detection

### D2R Memory (Maphack)
- Uses legacy D2R offsets (may break on 3.x patch)
- Requires community reverse-engineering for offset updates
- Vision-based alternative under investigation

### Not Implemented (Config-Only)
- **Cubing & runewords**: recipes defined in YAML, executor not built
- **Monster skip logic**: immunity/enchant skip list in config, not checked
- **Gambling/leveling**: AutoSkill/AutoStat in config, execution missing
- **Advanced pathing**: no A* or Pather (random walk instead)
- **Crafting system**: recipe system stubbed, not executed

---

## Future Roadmap

### v1.1 (Q1 2026)
- [ ] Implement Pather (A* on vision-detected map)
- [ ] Add cubing executor (recipe matching + ingredient detection)
- [ ] Monster skip logic (immunity check during combat)
- [ ] Multi-resolution support (800x600 → any resolution)

### v1.2 (Q2 2026)
- [ ] AutoSkill/AutoStat executor (point allocation on levelup)
- [ ] Gambling executor (Gheed gold management)
- [ ] Advanced loot evaluation (runeword bases, craft recipes)
- [ ] Waypoint caching (pre-learned map structure)

### v1.3+ (Future)
- [ ] Custom quest handling (Countess, Mephisto, Baal optimization)
- [ ] Rune/gem grinding (specific drop-seeking)
- [ ] Build respec automation (token/NPC interaction)
- [ ] Multi-game synchronization (if supporting multiple clients)

---

## Statistics

| Metric | Value |
|--------|-------|
| Rust LOC | ~11,400 (botter ~8,400 + maphack ~3,000) |
| JavaScript/CSS/HTML LOC | ~3,100 |
| YAML configs | 8 character presets |
| Test count | 192 (85+99+8) |
| Test pass rate | 100% |
| Build time | ~10s (release) |
| Frame buffer shards | 16 |
| FrameState size | ~192 bytes |
| Capture FPS target | 25 Hz |
| Decision rate | 25 Hz (40ms tick) |
| Input threads | 4 (round-robin) |
| NPC locations | 5 acts × 7 NPCs = 35 hardcoded |
| Attack slots | 7 (preattack + 3 target types × 2 timed variants) |
| Config sections | 18 |
| Supported classes | 7 (Sorceress, Paladin, Amazon, Necromancer, Assassin, Barbarian, Druid) |

---

## Contributors

- **Rust vision agent**: Built from scratch using DXGI, Windows API, pixel heuristics
- **Kolbot legacy**: 20+ years of combat/town/loot logic (D2BS JavaScript → Rust)
- **D2R research**: Community offsets, spell effects, item classification

---

## License

MIT License — see LICENSE file for details.

**Important**: This tool is for personal offline D2R use only. Respect Blizzard's Terms of Service.
