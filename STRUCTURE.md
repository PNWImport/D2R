# KillZBot Repository Structure

Complete guide to the codebase organization.

```
KillZBot/
├── README.md                 ← START HERE: Project overview
├── INSTALL.md                ← Setup & installation guide
├── CHANGELOG.md              ← Version history & milestones
├── STRUCTURE.md              ← This file
├── LICENSE                   ← MIT license
├── .gitignore                ← Git exclusions
│
├── install.ps1               ← Unified installer (Windows PowerShell)
├── install.bat               ← Batch wrapper for PowerShell
│
├── botter/                   ← Vision Agent (Rust, farming AI)
│   ├── Cargo.toml            ← Rust project config
│   ├── src/
│   │   ├── main.rs           ← Entry point, argument parsing, config loading
│   │   ├── lib.rs            ← Library exports
│   │   ├── config/
│   │   │   └── mod.rs        ← AgentConfig (18 config sections, YAML serde)
│   │   ├── decision/
│   │   │   ├── mod.rs        ← Module exports
│   │   │   ├── engine.rs     ← DecisionEngine (combat, survival, loot logic)
│   │   │   └── game_manager.rs ← GameManager (7-phase state machine)
│   │   ├── vision/
│   │   │   ├── mod.rs        ← Vision module exports
│   │   │   ├── shard_buffer.rs ← Lock-free FrameState buffer (16 shards)
│   │   │   ├── capture.rs    ← DXGI frame capture, vision pipeline
│   │   │   └── mod.rs        ← Vision system orchestration
│   │   ├── input/
│   │   │   ├── simulator.rs  ← Simulation stubs (Linux testing)
│   │   │   ├── windows_input.rs ← SendInput dispatch (Windows)
│   │   │   └── mod.rs        ← Input trait & types
│   │   ├── stealth/
│   │   │   ├── mod.rs        ← Stealth module
│   │   │   ├── capture_timing.rs ← 25 Hz frame capture timing controller
│   │   │   ├── thread_input.rs ← Thread-rotated input pool (4 workers)
│   │   │   ├── syscall_cadence.rs ← Syscall jitter for fingerprint breaking
│   │   │   ├── handle_table.rs ← Pseudo-handle obfuscation
│   │   │   └── process_identity.rs ← PEB disguise (Windows)
│   │   ├── native_messaging/
│   │   │   └── mod.rs        ← Chrome native messaging host (stdio protocol)
│   │   ├── training/
│   │   │   ├── logger.rs     ← Decision logging for analysis
│   │   │   └── mod.rs        ← Training module
│   │   └── tests/
│   │       └── *.rs          ← Integration tests (bin/stress)
│   │
│   ├── configs/              ← Character YAML templates (8 pre-configured)
│   │   ├── sorceress_blizzard.yaml
│   │   ├── sorceress_light.yaml
│   │   ├── paladin_hammerdin.yaml
│   │   ├── amazon_javazon.yaml
│   │   ├── necromancer_fishymancer.yaml
│   │   ├── assassin_trapsin.yaml
│   │   ├── barbarian_ww.yaml
│   │   └── druid_wind.yaml
│   │
│   ├── deploy/               ← Installation scripts & manifests
│   │   ├── install_host.ps1  ← Install native messaging host
│   │   ├── manifest.json     ← Native host manifest template
│   │   └── uninstall.ps1     ← Uninstall script
│   │
│   ├── target/               ← Rust build output (ignored)
│   │   ├── release/d2_vision_agent.exe
│   │   └── ...
│   │
│   └── tests/
│       ├── stress.rs         ← Stress tests (8 tests)
│       └── integration.rs    ← Integration tests (10+ tests)
│
├── maphack/                  ← Map Helper (Rust, memory-based map reader)
│   ├── Cargo.toml            ← Rust project config
│   ├── src/
│   │   ├── main.rs           ← Entry point, map memory reader
│   │   ├── lib.rs            ← Library exports
│   │   ├── memory/
│   │   │   ├── mod.rs        ← Memory reading interface
│   │   │   └── d2r_offsets.rs ← D2R memory structure offsets
│   │   ├── map/
│   │   │   ├── mod.rs        ← Map data structures
│   │   │   ├── tiles.rs      ← Tile parsing
│   │   │   └── objects.rs    ← Object/NPC parsing
│   │   ├── rendering/
│   │   │   └── mod.rs        ← Map rendering pipeline
│   │   └── native_messaging/
│   │       └── mod.rs        ← Chrome native messaging host
│   │
│   ├── target/               ← Rust build output (ignored)
│   │   ├── release/d2r_map_helper.exe
│   │   └── ...
│   │
│   └── configs/              ← Map configuration templates
│
├── extension/                ← Chrome Extension (MV3)
│   └── chrome_extension/
│       ├── manifest.json     ← Extension metadata & permissions
│       ├── background.js     ← Service worker (native host bridge)
│       ├── popup.html        ← Control panel UI
│       ├── popup.js          ← Control panel logic
│       ├── popup.css         ← Control panel dark theme
│       ├── map_content.js    ← Content script (map overlay injection)
│       ├── map_overlay.html  ← Map overlay HTML
│       └── map_overlay.css   ← Map overlay styles
│
├── kolbot/                   ← Classic D2 Bot (D2BS JavaScript reference)
│   ├── D2Bot.exe             ← Manager executable
│   ├── d2bs/
│   │   ├── D2BS.dll          ← D2BS engine
│   │   ├── api.html          ← API documentation (TiddlyWiki)
│   │   └── kolbot/           ← Bot library
│   │       ├── libs/         ← Core systems (Town, Pather, Pickit, etc.)
│   │       ├── tools/        ← Utility scripts
│   │       └── config/       ← Character configurations
│   │
│   ├── +setup/               ← Setup/installation scripts
│   │   ├── setup.ps1         ← PowerShell installer
│   │   ├── setup.bat         ← Batch wrapper
│   │   └── starter/          ← Starter config templates
│   │
│   └── docs/                 ← Documentation
│
└── extracted/                ← Dev extraction directory (ignored in release)
    └── ...
```

---

## Key Files & Their Purpose

### Core Entry Points
- **`botter/src/main.rs`** — Vision agent main loop
  - Argument parsing (config path selection)
  - DXGI capture initialization
  - Frame loop (25 Hz tick)
  - Native messaging host connection
  - Signal handling (graceful shutdown)

- **`maphack/src/main.rs`** — Map helper main loop
  - D2R memory reading
  - Map data parsing
  - Native messaging host connection

- **`extension/chrome_extension/background.js`** — Chrome service worker
  - Manages two native messaging hosts
  - Bridges Chrome UI ↔ Agent communication
  - Stats caching and event handling

### Decision & Logic
- **`botter/src/decision/engine.rs`** (1200 LOC)
  - Priority-based decision system
  - Survival checks (chicken, potions, TP)
  - Combat logic (dodge, static field, attack slots)
  - Attack target derivation (Boss/Champion/Normal/Immune)
  - Humanization (delays, variance, missed clicks)

- **`botter/src/decision/game_manager.rs`** (900 LOC)
  - 7-phase state machine (OutOfGame → Farming → Exit)
  - Town automation (NPC sequences)
  - Game lifecycle (exit, inter-game delays)
  - Per-act NPC coordinates

### Vision & Capture
- **`botter/src/vision/capture.rs`** (600 LOC)
  - Frame extraction from DXGI screenshot
  - Enemy detection (nearest, health %, type)
  - Loot detection (item quality, position)
  - Buff/debuff detection (visual indicators)
  - Merc HP, belt potions, inventory status

- **`botter/src/vision/shard_buffer.rs`** (300 LOC)
  - Lock-free 16-shard FrameState buffer
  - Producer (capture thread) → Consumer (decision thread)
  - ABA-protected concurrent reads

### Configuration
- **`botter/src/config/mod.rs`** (835 LOC)
  - AgentConfig struct with 18 sections
  - Serde YAML serialization/deserialization
  - serde(default) for backward-compatibility
  - 8 pre-configured character YAMLs

### Stealth & Input
- **`botter/src/stealth/thread_input.rs`** (300 LOC)
  - Thread-rotated 4-worker input pool
  - Per-thread jitter on SendInput calls
  - Round-robin dispatch

- **`botter/src/stealth/capture_timing.rs`** (250 LOC)
  - 25 Hz frame capture timing
  - Skip/burst mode for dynamic frame rate
  - Timing jitter

- **`botter/src/stealth/process_identity.rs`** (150 LOC)
  - PEB disguise (Windows, reports as NetworkService)
  - Command-line spoofing

- **`botter/src/stealth/syscall_cadence.rs`** (200 LOC)
  - Decoy syscall injection
  - Breaks statistical fingerprinting

### Native Messaging
- **`botter/src/native_messaging/mod.rs`** (400 LOC)
  - Chrome native messaging protocol (4-byte LE length + JSON)
  - Commands: pause, resume, get_stats, update_config, shutdown
  - Stats struct (SharedAgentStats with atomics)

### Chrome Extension
- **`extension/chrome_extension/popup.html`** (50 LOC)
  - Control panel layout
  - Status indicators, buttons, stats display

- **`extension/chrome_extension/popup.js`** (150 LOC)
  - Real-time stats polling (2s interval)
  - Pause/resume, config selection
  - Map overlay controls

- **`extension/chrome_extension/background.js`** (300 LOC)
  - Native host connection management
  - Stats caching, request timeouts
  - Command routing

### Testing
- **`botter/tests/stress.rs`** (700 LOC)
  - 8 stress tests
  - 10s sustained loops, lock-free buffer stress
  - Thread pool throughput testing

- **Unit tests** (scattered throughout src/)
  - 85 library tests
  - 97 binary integration tests
  - 190 total, all passing

---

## Configuration Hierarchy

```
AgentConfig (root)
├── character_class: String (Sorceress, Paladin, etc.)
├── build: String (blizzard, hammerdin, etc.)
├── survival: SurvivalConfig
│   ├── chicken_hp_pct: u8
│   ├── hp_potion_pct: u8
│   ├── mana_potion_pct: u8
│   └── ...
├── combat: CombatConfig
│   ├── attack_slots: AttackSlots (7 slots)
│   ├── primary_skill_key: char
│   ├── dodge: bool
│   ├── static_field: StaticFieldConfig
│   └── ...
├── loot: LootConfig
├── town: TownConfig
│   ├── task_order: Vec<String>
│   ├── go_to_town_triggers: TownTriggers
│   └── stash_rules: StashRules
├── buffs: Vec<BuffConfig>
├── humanization: HumanizationConfig
├── session: SessionConfig
├── farming: FarmingConfig
│   └── sequence: Vec<FarmRun>
├── leveling: LevelingConfig
├── cubing: CubingConfig
├── runewords: RunewordConfig
├── gambling: GamblingConfig
├── class_specific: ClassSpecificConfig
├── monster_skip: MonsterSkipConfig
├── clear: ClearConfig
├── merc: MercConfig
└── inventory: InventoryConfig
```

---

## Build & Test Commands

```bash
# Build
cd botter
cargo build --release              # Vision agent
cd ../maphack
cargo build --release              # Map helper

# Test
cd ../botter
cargo test                          # All tests (190)
cargo test decision::               # Decision tests only
cargo test game_manager::           # Game lifecycle tests
cargo test --test stress            # Stress tests (8)

# Lint
cargo clippy --all
cargo fmt --check

# Documentation
cargo doc --open
```

---

## NPC Coordinates (Per-Act)

All hardcoded at 800x600 base resolution (scales with math):

**Act 1 (Rogue Encampment)**
- Akara (healer): (155, 72)
- Charsi (repair): (257, 209)
- Kashya (merc): (466, 236)
- Cain (identify): Akara location if not yet rescued
- Stash: (127, 237)

**Act 2 (Lut Gholein)**
- Fara (healer/repair): (260, 142)
- Drognan (potion vendor): (196, 93)
- Greiz (merc): (457, 218)
- Stash: (230, 290)

**Act 3 (Kurast)**
- Ormus (healer/potion): (307, 170)
- Hratli (repair): (226, 63)
- Asheara (merc): (408, 95)
- Stash: (166, 310)

**Act 4 (Pandemonium Fortress)**
- Jamella (healer/potion): (152, 107)
- Halbu (repair): (181, 155)
- Tyrael (merc): (152, 107)
- Stash: (186, 246)

**Act 5 (Harrogath)**
- Malah (healer/potion): (328, 63)
- Larzuk (repair): (135, 142)
- Qual-Kehk (merc): (458, 147)
- Anya (identify): (385, 154)
- Stash: (306, 266)

---

## Development Workflow

1. **Make changes** to Rust code in `botter/src/` or `maphack/src/`
2. **Run tests**: `cargo test` (all tests should pass)
3. **Check lints**: `cargo clippy`
4. **Format code**: `cargo fmt`
5. **Commit**: `git commit -m "Clear message"`
6. **Push**: `git push origin claude/prepare-kolbot-production-zGrdr`

---

## Common Tasks

### Add a new config section
1. Define struct in `botter/src/config/mod.rs`
2. Add `#[serde(default)]` for backward-compatibility
3. Add to `AgentConfig` struct
4. Implement `Default` trait
5. Add tests for serialization round-trip

### Add a new decision check
1. Implement logic in `botter/src/decision/engine.rs`
2. Call from `DecisionEngine::decide()` in priority order
3. Return `Decision { action, delay, priority, reason }`
4. Add test case with mock FrameState

### Add a new FrameState field
1. Add field to `FrameState` struct in `botter/src/vision/shard_buffer.rs`
2. Initialize in `FrameState::default()`
3. Populate in vision pipeline (`botter/src/vision/capture.rs`)
4. Update FrameState size test (must stay < 256 bytes)
5. Use in decision engine as needed

---

## Release Checklist

- [ ] All tests pass (190/190)
- [ ] No clippy warnings
- [ ] Code formatted (`cargo fmt`)
- [ ] Documentation updated (README, CHANGELOG)
- [ ] README has correct version number
- [ ] CHANGELOG has entry for new version
- [ ] Git history is clean
- [ ] Version bumped in Cargo.toml and manifest.json
- [ ] Built binaries tested on Windows
- [ ] Installer script tested
- [ ] Chrome extension loads without errors
- [ ] Native hosts register correctly
- [ ] Sample configs work end-to-end

---

## Architecture Decisions

### Why Rust?
- Performance (25 Hz frame capture + decision in single thread)
- Memory safety (no buffer overflows, data races caught at compile-time)
- Cross-platform (maphack in progress for non-Windows)
- Native Windows API bindings (DXGI, SendInput, Registry)

### Why Chrome Extension?
- Legitimate subprocess (native messaging = Chrome child process)
- No injection, no hooks, no DLL mapping
- Can disguise as Chrome utility (PEB spoofing + syscall jitter)
- Provides control panel UI for free (browser UI framework)

### Why Lock-Free Buffer?
- 25 Hz capture thread ≠ decision thread frequency
- Capture must never block (could miss frames)
- Decision must always get latest frame (no buffering)
- Lock-free = zero contention, deterministic latency

### Why Per-Act NPC Coordinates?
- D2R has fixed NPC placement per act
- Hardcoding avoids memory reads (pure vision)
- Scales with game resolution via math
- Future: dynamic NPC detection via vision

---

## Useful References

- **D2R Memory Structure**: See `maphack/src/memory/d2r_offsets.rs`
- **Kolbot Docs**: `kolbot/d2bs/api.html`
- **D2 Forums**: Community reverse-engineering threads
- **Chrome Native Messaging**: [Google Docs](https://developer.chrome.com/docs/extensions/mv3/nativeMessaging/)

---

Done! Full repo structure documented. 🎯
