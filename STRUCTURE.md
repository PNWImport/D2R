# KZB Repository Structure

Complete guide to the codebase organization.

```
KZB/
├── README.md                 ← START HERE: Project overview
├── INDEX.md                  ← Quick-reference index
├── INSTALL.md                ← Setup & installation guide
├── QUICKSTART.md             ← Getting started fast
├── CHANGELOG.md              ← Version history & milestones
├── STRUCTURE.md              ← This file
├── .gitignore                ← Git exclusions
│
├── install.ps1               ← Unified installer (PowerShell) + Leatrix TCP optimization
├── install.bat               ← Batch wrapper for PowerShell
├── latency_profile.py        ← Monte Carlo latency profiler (50K runs, V1 vs V2)
├── LATENCY_ANALYSIS.md       ← Config update pipeline latency analysis
│
├── assets/
│   ├── kzb_header.webp       ← Project header image
│   └── .gitkeep
│
├── vision/                   ← Vision Agent (Rust, ~8400 LOC, farming AI)
│   ├── Cargo.toml            ← Rust project config
│   ├── Cargo.lock            ← Dependency lock file
│   ├── build.rs              ← Build script
│   ├── src/
│   │   ├── main.rs           ← Entry point, argument parsing, config loading
│   │   ├── lib.rs            ← Library exports
│   │   ├── host_registry.rs  ← Native messaging host registry helpers
│   │   ├── config/
│   │   │   └── mod.rs        ← AgentConfig (18 config sections, YAML serde)
│   │   ├── decision/
│   │   │   ├── mod.rs        ← Module exports
│   │   │   ├── engine.rs     ← DecisionEngine (combat, survival, loot logic)
│   │   │   ├── game_manager.rs ← GameManager (7-phase state machine)
│   │   │   ├── quad_cache.rs ← QuadCache 4-lane acceleration (O(1) decisions)
│   │   │   ├── progression.rs ← Quest state, difficulty progression, script sequence
│   │   │   └── script_executor.rs ← Script step execution with visual cue verification
│   │   ├── vision/
│   │   │   ├── mod.rs        ← Vision module exports
│   │   │   ├── capture.rs    ← DXGI frame capture, vision pipeline
│   │   │   └── shard_buffer.rs ← Lock-free FrameState buffer (16 shards)
│   │   ├── input/
│   │   │   ├── mod.rs        ← Input trait & types
│   │   │   └── simulator.rs  ← Simulation stubs (Linux testing)
│   │   ├── stealth/
│   │   │   ├── mod.rs        ← Stealth module
│   │   │   ├── capture_timing.rs ← 25 Hz frame capture timing controller
│   │   │   ├── handle_table.rs ← Pseudo-handle obfuscation
│   │   │   ├── process_identity.rs ← PEB disguise (Windows)
│   │   │   ├── syscall_cadence.rs ← Syscall jitter for fingerprint breaking
│   │   │   └── thread_input.rs ← Thread-rotated input pool (4 workers)
│   │   ├── native_messaging/
│   │   │   └── mod.rs        ← Chrome native messaging host (stdio protocol)
│   │   └── training/
│   │       ├── mod.rs        ← Training module
│   │       └── logger.rs     ← Decision logging for analysis
│   │
│   ├── configs/              ← Character YAML templates (8 pre-configured)
│   │   ├── amazon_javazon.yaml
│   │   ├── assassin_trapsin.yaml
│   │   ├── barbarian_whirlwind.yaml
│   │   ├── druid_wind.yaml
│   │   ├── necromancer_fishymancer.yaml
│   │   ├── paladin_hammerdin.yaml
│   │   ├── sorceress_blizzard.yaml
│   │   └── sorceress_meteorb.yaml
│   │
│   ├── deploy/               ← Installation scripts & manifests
│   │   ├── default_config.yaml       ← Default configuration template
│   │   ├── install_host.ps1          ← Install native messaging host
│   │   └── native_host_manifest.json ← Native host manifest template
│   │
│   ├── tests/
│   │   └── stress.rs         ← Stress tests (8 tests)
│   │
│   └── benches/
│       └── shard_bench.rs    ← Shard buffer benchmarks
│
├── overlay/                  ← Map Helper (Rust, ~3000 LOC, memory-based map reader)
│   ├── Cargo.toml            ← Rust project config
│   ├── Cargo.lock            ← Dependency lock file
│   ├── offsets.json.example  ← Example memory offsets file
│   ├── src/
│   │   ├── main.rs           ← Entry point, map memory reader
│   │   ├── discovery.rs      ← Process/window discovery
│   │   ├── host_registry.rs  ← Native messaging host registry helpers
│   │   ├── mapgen.rs         ← Map generation / tile assembly
│   │   ├── memory.rs         ← Memory reading interface
│   │   ├── offsets.rs        ← Game memory structure offsets
│   │   ├── protocol.rs       ← Native messaging protocol handling
│   │   └── stealth/
│   │       ├── mod.rs        ← Stealth module (flat file, not a subdir tree)
│   │       ├── process_identity.rs ← PEB disguise
│   │       └── syscall_cadence.rs  ← Syscall jitter
│   │
│   ├── installer/
│   │   └── install_map_host.ps1 ← Map host installer script
│   │
│   └── tests/
│       ├── protocol_test.py  ← Protocol integration tests (Python)
│       └── verify.py         ← Verification tests (Python)
│
├── extension/                ← Chrome Extension (MV3, v1.4.0, ~3100 LOC)
│   ├── test_gui.html         ← GUI test harness
│   ├── test_serve.sh         ← Local test server script
│   └── chrome_extension/
│       ├── manifest.json     ← Extension metadata & permissions (v1.4.0)
│       ├── background.js     ← Service worker (native host bridge, 375 LOC)
│       ├── popup.html        ← Control panel UI (1521 LOC, 11 tabs, 503 settings, 77 scripts)
│       ├── popup.js          ← Control panel logic (372 LOC)
│       ├── popup.css         ← Control panel dark theme (615 LOC)
│       ├── map_content.js    ← Content script (map overlay injection, 260 LOC)
│       └── kzb_header.webp   ← Extension header image
│
└── kolbot/                   ← Legacy engine + scripts
    ├── D2Bot.exe             ← Manager executable (4.5 MB)
    ├── setup.bat             ← Batch setup wrapper
    ├── update.bat            ← Update & submodule init script
    ├── README.md             ← Kolbot quickstart guide
    ├── package.json          ← Node.js dependencies
    ├── tsconfig.json         ← TypeScript config
    ├── biome.json            ← Biome linter config
    ├── .eslintrc.js          ← ESLint rules
    ├── .editorconfig         ← Editor formatting rules
    ├── .gitignore            ← Git exclusions
    ├── .github/
    │   ├── copilot-instructions.md    ← AI coding guidelines
    │   ├── ISSUE_TEMPLATE/bug_report.md
    │   ├── instructions/typescript.instructions.md
    │   └── workflows/eslint.yml       ← CI linting
    ├── .vscode/
    │   ├── settings.json     ← VS Code workspace settings
    │   └── extensions.json   ← Recommended extensions
    ├── data/                 ← Runtime data directory
    │   └── .gitkeep
    ├── logs/                 ← Runtime logs directory
    │   └── .gitkeep
    │
    ├── d2bs/                 ← Script engine (~35 MB)
    │   ├── D2BS.dll          ← Native engine DLL
    │   ├── HISTORY.txt       ← Version history
    │   ├── LICENSE.rtf       ← License (RTF format)
    │   ├── LICENSE.txt       ← License (text format)
    │   ├── api.html          ← API reference (TiddlyWiki)
    │   └── kolbot/           ← Kolbot scripts & library
    │       ├── D2BotBlank.dbj              ← Blank template entry
    │       ├── D2BotChannel.dbj            ← Channel bot entry
    │       ├── D2BotCleaner.dbj           ← Area cleaner entry
    │       ├── D2BotCharRefresher.dbj     ← Character refresh entry
    │       ├── D2BotFollow.dbj            ← Follower bot entry
    │       ├── D2BotGameAction.dbj        ← Game action entry
    │       ├── D2BotLead.dbj              ← Lead control entry
    │       ├── D2BotMap.dbj               ← Map mode entry
    │       ├── D2BotMule.dbj              ← Mule manager entry
    │       ├── D2BotPubJoin.dbj           ← Public game joiner entry
    │       ├── D2BotSoloPlay.dbj          ← LEVELBOT ENTRY POINT ★
    │       ├── default.dbj                ← Default game script entry
    │       ├── console/                   ← Console interface scripts
    │       ├── data/                      ← Runtime config/data storage
    │       ├── libs/
    │       │   ├── core/                  ← Core framework (~2000 LOC)
    │       │   │   ├── Attack.js          ← Attack system
    │       │   │   ├── Common.js          ← Common routines
    │       │   │   ├── Config.js          ← Config loader
    │       │   │   ├── Cubing.js          ← Cube recipes
    │       │   │   ├── Experience.js      ← EXP tracking & leveling math
    │       │   │   ├── Loader.js          ← Script loader
    │       │   │   ├── Me.js              ← Character stats
    │       │   │   ├── Misc.js            ← Misc utilities
    │       │   │   ├── NPC.js             ← NPC interactions
    │       │   │   ├── Pather.js          ← Pathfinding
    │       │   │   ├── Pickit.js          ← Item picking
    │       │   │   ├── Precast.js         ← Spell precasting
    │       │   │   ├── Skill.js           ← Skill system
    │       │   │   ├── Storage.js         ← Inventory management
    │       │   │   ├── Town.js            ← Town automation
    │       │   │   ├── Util.js            ← Utilities
    │       │   │   ├── Attacks/           ← Class-specific attacks (7 classes)
    │       │   │   ├── Auto/              ← Auto systems (Build, Skill, Stat)
    │       │   │   ├── Common/            ← Common routines
    │       │   │   ├── GameData/          ← Game databases
    │       │   │   └── ...
    │       │   ├── config/                ← Character configs
    │       │   │   ├── Amazon.js
    │       │   │   ├── Assassin.js
    │       │   │   ├── Barbarian.js
    │       │   │   ├── Druid.js
    │       │   │   ├── Necromancer.js
    │       │   │   ├── Paladin.js
    │       │   │   ├── Sorceress.js
    │       │   │   ├── Builds/            ← Build templates
    │       │   │   └── Templates/         ← Config templates
    │       │   ├── modules/               ← Async, events, workers
    │       │   ├── scripts/               ← 85+ farming/leveling scripts ★
    │       │   │   ├── AutoBaal.js        ← Auto Baal runs
    │       │   │   ├── AutoChaos.js       ← Auto Chaos/Diablo runs
    │       │   │   ├── Baal.js            ← Baal run (primary leveling)
    │       │   │   ├── Cows.js            ← Cow levels (early leveling)
    │       │   │   ├── Questing.js        ← Quest progression
    │       │   │   ├── Rushee.js          ← Rush completion
    │       │   │   ├── Pindleskin.js
    │       │   │   ├── Mephisto.js
    │       │   │   ├── Nihlathak.js
    │       │   │   ├── Countess.js
    │       │   │   ├── Andariel.js
    │       │   │   └── ... 70+ more ...
    │       │   ├── SoloPlay/              ← SOLOPLAY LEVELBOT (251 files) ★★
    │       │   │   ├── SoloPlay.js        ← Entry point
    │       │   │   ├── OOG/               ← Out-of-game logic
    │       │   │   │   └── SoloEntry.js   ← Main leveling controller
    │       │   │   ├── BuildFiles/        ← Class-specific builds & runewords
    │       │   │   │   ├── amazon/        ← Amazon builds
    │       │   │   │   ├── assassin/      ← Assassin builds
    │       │   │   │   ├── barbarian/     ← Barbarian builds
    │       │   │   │   ├── druid/         ← Druid builds
    │       │   │   │   ├── necromancer/   ← Necromancer builds
    │       │   │   │   ├── paladin/       ← Paladin builds
    │       │   │   │   ├── sorceress/     ← Sorceress builds (Blizz, Meteor, Light, etc.)
    │       │   │   │   └── Runewords/     ← Runeword definitions
    │       │   │   ├── Config/            ← Build configurations
    │       │   │   ├── Core/              ← SoloPlay core library
    │       │   │   ├── Modules/           ← SoloPlay modules
    │       │   │   ├── Settings/          ← Leveling settings
    │       │   │   ├── Threads/           ← Threading logic
    │       │   │   ├── Tools/             ← Utility tools
    │       │   │   └── Workers/           ← Worker threads
    │       │   ├── systems/               ← Bot systems
    │       │   │   ├── automule/          ← Auto mule system
    │       │   │   ├── autorush/          ← Auto rush system
    │       │   │   ├── charrefresher/     ← Character refresh
    │       │   │   ├── cleaner/           ← Area cleaner
    │       │   │   ├── crafting/          ← Crafting system
    │       │   │   ├── gameaction/        ← Game action handler
    │       │   │   ├── gambling/          ← Gambling system
    │       │   │   └── torch/             ← Torch system
    │       │   ├── oog/                   ← Out-of-game library
    │       │   ├── manualplay/            ← Manual play mode hooks
    │       │   ├── sdk/                   ← SDK definitions
    │       │   └── OOG.js, Polyfill.js, json2.js, require.js
    │       ├── mules/                     ← Mule character data
    │       ├── logs/                      ← Runtime logs
    │       ├── pickit/                    ← Item picker configs
    │       │   ├── kolton.nip             ← Main pickit (59 KB)
    │       │   ├── LLD.nip                ← Low-level dueling items
    │       │   ├── classic.nip            ← Classic items
    │       │   └── ... 6 more ...
    │       ├── threads/                   ← Game scripts
    │       │   ├── HeartBeat.js           ← Main loop
    │       │   ├── Party.js               ← Party coordination
    │       │   ├── AntiHostile.js         ← Anti-hostile control
    │       │   └── ... more ...
    │       ├── sdk/                       ← SDK definitions
    │       └── ...
    │
    ├── limedrop/             ← Drop notification system (15 files)
    │   ├── index.html        ← Drop notification viewer
    │   ├── js/
    │   ├── css/
    │   └── assets/
    │
    └── +setup/               ← Setup templates & configs
        ├── data/
        │   ├── cdkeys.json   ← CD keys placeholder
        │   └── .gitkeep
        ├── logs/             ← Log templates
        │   ├── Console.rtf
        │   ├── exceptions.log
        │   └── keyinfo.log
        ├── d2bs.ini          ← D2BS engine configuration
        ├── setup.ps1         ← PowerShell installer script
        ├── starter/          ← Global starter configs
        │   ├── StarterConfig.js  ← Connection & timing settings
        │   └── AdvancedConfig.js ← Profile-specific settings
        ├── automule/         ← Mule system configs
        ├── channel/          ← Channel configs
        ├── charrefresher/    ← Character refresh configs
        ├── cleaner/          ← Cleaner system configs
        ├── config/           ← Character build configs
        ├── crafting/         ← Crafting system configs
        ├── follow/           ← Follower configs
        ├── gambling/         ← Gambling system configs
        ├── gameaction/       ← Game action configs
        ├── lead/             ← Lead control configs
        ├── mulelogger/       ← Mule logging configs
        ├── pubjoin/          ← Public game join configs
        └── torch/            ← Torch system configs
```

---

## Key Files & Their Purpose

### Core Entry Points
- **`vision/src/main.rs`** -- Vision agent main loop
  - Argument parsing (config path selection)
  - DXGI capture initialization
  - Frame loop (25 Hz tick) with dual tick drain
  - Native messaging host connection
  - Direct JSON deserialization (`serde_json::from_value`)
  - Signal handling (graceful shutdown)

- **`overlay/src/main.rs`** -- Map helper main loop
  - Game process discovery and memory reading
  - Map data parsing and generation
  - Native messaging host connection

- **`extension/chrome_extension/background.js`** (375 LOC) -- Chrome service worker
  - Manages two native messaging hosts
  - Bridges Chrome UI <-> Agent communication
  - Stats caching and event handling

- **`kolbot/D2Bot.exe`** -- Kolbot manager (4.5 MB)
  - D2Bot instance manager and game controller
  - Multi-profile game creation and automation
  - Entry point selection (default.dbj, D2BotSoloPlay.dbj, etc.)

- **`kolbot/d2bs/kolbot/D2BotSoloPlay.dbj`** -- **LEVELBOT ENTRY POINT** ★
  - Launcher for SoloPlay automated leveling system
  - Delegates to SoloPlay/OOG/SoloEntry.js
  - Handles 1-99 progression across all difficulties

- **`kolbot/d2bs/kolbot/libs/SoloPlay/OOG/SoloEntry.js`** -- **MAIN LEVELBOT CONTROLLER** ★★
  - Orchestrates complete leveling automation
  - Handles game creation, progression strategy, leveling thresholds
  - Manages character skill/stat advancement
  - Coordinates multi-phase leveling (Normal -> Nightmare -> Hell)

### Decision & Logic
- **`vision/src/decision/engine.rs`**
  - Priority-based decision system
  - Survival checks via ThresholdBins (O(1) flat field reads, no config traversal)
  - Combat logic (dodge, static field, attack slots)
  - Attack target derivation (Boss/Champion/Normal/Immune)
  - Humanization (delays, variance, missed clicks)

- **`vision/src/decision/game_manager.rs`**
  - 7-phase state machine (OutOfGame -> Farming -> Exit)
  - Town automation (NPC sequences)
  - Game lifecycle (exit, inter-game delays)
  - Per-act NPC coordinates
  - QuadCache warm/reload integration

- **`vision/src/decision/quad_cache.rs`** — **QuadCache four-lane acceleration**
  - Lane 2: PreparedRun indexed at startup (farm scripts, act derivation, boss detection)
  - Lane 3: ThresholdBins — 12 survival fields flattened for O(1) reads
  - Lane 4: HotKey (HpBin × combat × loot) hit counters + SpanFeatures for LLM
  - ~22 KB total footprint, all agent-private heap
  - `warm()` at startup (~5μs), `reload_thresholds()` / `rewarm_runs()` on config change

- **`vision/src/decision/progression.rs`**
  - Quest state tracking and difficulty progression
  - Script sequence with area/quest/boss steps
  - Visual cue detection for script advancement

- **`vision/src/decision/script_executor.rs`**
  - Script step execution engine
  - Visual cue verification before step completion

### Vision & Capture
- **`vision/src/vision/capture.rs`**
  - Frame extraction from DXGI screenshot
  - Enemy detection (nearest, health %, type)
  - Loot detection (item quality, position)
  - Buff/debuff detection (visual indicators)
  - Merc HP, belt potions, inventory status

- **`vision/src/vision/shard_buffer.rs`**
  - Lock-free 16-shard FrameState buffer
  - Producer (capture thread) -> Consumer (decision thread)
  - ABA-protected concurrent reads

### Configuration
- **`vision/src/config/mod.rs`**
  - AgentConfig struct with 18 sections
  - Serde YAML serialization/deserialization
  - serde(default) for backward-compatibility
  - 8 pre-configured character YAMLs

### Stealth & Input
- **`vision/src/stealth/thread_input.rs`**
  - Thread-rotated 4-worker input pool
  - Per-thread jitter on SendInput calls
  - Round-robin dispatch

- **`vision/src/stealth/capture_timing.rs`**
  - 25 Hz frame capture timing
  - Skip/burst mode for dynamic frame rate
  - Timing jitter

- **`vision/src/stealth/process_identity.rs`**
  - PEB disguise (Windows, reports as NetworkService)
  - Command-line spoofing

- **`vision/src/stealth/syscall_cadence.rs`**
  - Decoy syscall injection
  - Breaks statistical fingerprinting

- **`vision/src/stealth/handle_table.rs`**
  - Pseudo-handle obfuscation

- **`vision/src/input/mod.rs`**
  - Input trait definition and types

- **`vision/src/input/simulator.rs`**
  - Simulation stubs for Linux-based testing/development

### Native Messaging
- **`vision/src/native_messaging/mod.rs`**
  - Chrome native messaging protocol (4-byte LE length + JSON)
  - Commands: pause, resume, get_stats, update_config, shutdown
  - Stats struct (SharedAgentStats with atomics)
  - Responses built directly with `json!()` macros (no intermediary struct)

### Chrome Extension
- **`extension/chrome_extension/popup.html`** (1521 LOC)
  - 11-tab control panel layout
  - 503 settings across all tabs
  - 77 kolbot script configurations
  - Status indicators, buttons, stats display

- **`extension/chrome_extension/popup.js`** (372 LOC)
  - Real-time stats polling (2s interval)
  - Pause/resume, config selection
  - Map overlay controls

- **`extension/chrome_extension/popup.css`** (615 LOC)
  - Dark theme styling for all 11 tabs

- **`extension/chrome_extension/background.js`** (375 LOC)
  - Native host connection management
  - Stats caching, request timeouts
  - Command routing

- **`extension/chrome_extension/map_content.js`** (260 LOC)
  - Content script injected into page
  - Map overlay rendering

### Kolbot Leveling System
- **`kolbot/d2bs/kolbot/libs/SoloPlay/`** (251 files) -- **COMPLETE LEVELBOT**
  - Full automated 1-99 leveling system
  - Handles game creation, progression strategy, skill/stat allocation
  - All 7 classes with multiple build paths per class

- **`kolbot/d2bs/kolbot/libs/SoloPlay/BuildFiles/`**
  - Per-class build definitions with runewords
  - Example: `sorceress/` has 9 builds (Blizzard, Meteorb, Lightning, Ensorb, Blova, Cold, Start, Stepping, Leveling)
  - Each class has Start, Stepping, and Leveling builds for progression

- **`kolbot/d2bs/kolbot/libs/core/Experience.js`**
  - EXP tracking system with all 99 level thresholds
  - Functions: `progress()`, `gain()`, `gainPercent()`, `runsToLevel()`, `timeToLevel()`, `log()`
  - Core math for leveling progression calculations

- **`kolbot/d2bs/kolbot/libs/scripts/`** (85+ scripts)
  - **Leveling-focused:** Baal.js, AutoBaal.js, AutoChaos.js, Cows.js, Questing.js, Rushee.js
  - **Boss scripts:** Mephisto.js, Diablo.js, Nihlathak.js, Pindleskin.js, Countess.js, Andariel.js
  - **Leech/helper scripts:** BaalHelper.js, DiabloHelper.js, SealLeecher.js, MFHelper.js
  - **Utility:** Gamble.js, Crafting.js, GetEssences.js, GetFade.js, TownChicken.js

- **`kolbot/d2bs/kolbot/libs/config/`**
  - Character build templates (Amazon.js, Assassin.js, Barbarian.js, Druid.js, Necromancer.js, Paladin.js, Sorceress.js)
  - Build inheritance system with base config template

- **`kolbot/d2bs/kolbot/libs/systems/`**
  - **automule/** - Auto mule system for gear management
  - **autorush/** - Auto rush system for fast progression
  - **crafting/**, **gambling/** - Crafting and gambling automation
  - **torch/** - Torch farming system

- **`kolbot/+setup/starter/StarterConfig.js`**
  - Global connection settings (server IP, character name, password)
  - Game creation timing and delays
  - Profile management

- **`kolbot/+setup/starter/AdvancedConfig.js`**
  - Per-profile character selection and build
  - Game difficulty selection (Normal, Nightmare, Hell)
  - Leveling strategy and run selection

- **`kolbot/pickit/kolton.nip`** (59 KB)
  - Main item picker configuration
  - Item quality and type filtering

### Testing
- **`vision/tests/stress.rs`**
  - 8 stress tests
  - 10s sustained loops, lock-free buffer stress
  - Thread pool throughput testing

- **`vision/benches/shard_bench.rs`**
  - Shard buffer performance benchmarks

- **`overlay/tests/protocol_test.py`**
  - Protocol integration tests (Python)

- **`overlay/tests/verify.py`**
  - Maphack verification tests (Python)

- **Unit tests** (scattered throughout src/)
  - 130 library tests
  - 144 binary integration tests
  - 8 stress tests
  - 282 total

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
cd vision
cargo build --release              # Vision agent
cd ../maphack
cargo build --release              # Map helper

# Test
cd ../botter
cargo test                          # All tests (282)
cargo test decision::               # Decision tests only
cargo test game_manager::           # Game lifecycle tests
cargo test --test stress            # Stress tests (8)

# Bench
cargo bench                         # Run benchmarks

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

## Codebase Statistics

| Component | Language | LOC | Files |
|-----------|----------|-----|-------|
| vision | Rust | ~8,400 | 15 |
| overlay | Rust | ~3,000 | 10 |
| **Total Rust** | | **~11,400** | **25** |
| extension | JS/CSS/HTML | ~3,100 | 11 |
| kolbot core | JavaScript | ~2,000 | 30+ |
| kolbot scripts | JavaScript | ~70,000 | 85+ |
| **SoloPlay levelbot** | **JavaScript** | **~15,000** | **251** |
| **Total Project** | | **~100,000+** | **400+** |

| Test Suite | Count |
|------------|-------|
| Library unit tests | 130 |
| Binary integration tests | 144 |
| Stress tests | 8 |
| **Total** | **282** |

---

## Development Workflow

1. **Make changes** to Rust code in `vision/src/` or `overlay/src/`
2. **Run tests**: `cargo test` (all 282 tests should pass)
3. **Check lints**: `cargo clippy`
4. **Format code**: `cargo fmt`
5. **Commit**: `git commit -m "Clear message"`

---

## Kolbot Levelbot Setup

**To use the levelbot (SoloPlay) on your local machine:**

1. **Clone/pull the repository**
   ```bash
   git clone <repo-url> KZB
   cd KZB/kolbot
   ```

2. **Initialize submodules** (downloads SoloPlay + limedrop)
   ```bash
   update.bat
   # or manually:
   git submodule update --init --recursive
   ```

3. **Configure game installation path**
   - Edit `+setup/d2bs.ini` - Set `D2RPath` to your game installation directory
   - Edit `+setup/starter/StarterConfig.js` - Set server IP and account details

4. **Create a leveling character**
   - Edit `+setup/starter/AdvancedConfig.js` - Select character name and class
   - Copy appropriate class config to `d2bs/kolbot/libs/config/YourClass.YourChar.js`
   - Example: `Sorceress.LevelBot.js` for a Sorceress named "LevelBot"

5. **Select leveling build**
   - In AdvancedConfig.js, choose your build strategy
   - SoloPlay automatically selects optimal builds per level (Start -> Stepping -> Leveling)

6. **Run the levelbot**
   - Double-click `setup.bat` to initialize D2BS
   - Select your character profile in D2Bot manager
   - Start with `D2BotSoloPlay.dbj` entry point
   - Monitor progress in bot console

**Key configuration files:**
- `+setup/starter/StarterConfig.js` - Connection settings, game creation delays
- `+setup/starter/AdvancedConfig.js` - Profile selection, character build, difficulty
- `d2bs/kolbot/libs/config/YourClass.js` - Character build and attack patterns
- `pickit/kolton.nip` - Item picking rules

---

## Common Tasks

### Add a new config section
1. Define struct in `vision/src/config/mod.rs`
2. Add `#[serde(default)]` for backward-compatibility
3. Add to `AgentConfig` struct
4. Implement `Default` trait
5. Add tests for serialization round-trip

### Add a new decision check
1. Implement logic in `vision/src/decision/engine.rs`
2. Call from `DecisionEngine::decide()` in priority order
3. Return `Decision { action, delay, priority, reason }`
4. Add test case with mock FrameState

### Add a new FrameState field
1. Add field to `FrameState` struct in `vision/src/vision/shard_buffer.rs`
2. Initialize in `FrameState::default()`
3. Populate in vision pipeline (`vision/src/vision/capture.rs`)
4. Update FrameState size test (must stay < 256 bytes)
5. Use in decision engine as needed

---

## Release Checklist

- [ ] All tests pass (282/282)
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
- 25 Hz capture thread != decision thread frequency
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

- **Game Offsets**: See `overlay/src/offsets.rs` and `overlay/offsets.json.example`
- **Kolbot Docs**: `kolbot/d2bs/api.html`
- **D2 Forums**: Community reverse-engineering threads
- **Chrome Native Messaging**: [Google Docs](https://developer.chrome.com/docs/extensions/mv3/nativeMessaging/)
