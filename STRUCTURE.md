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
├── install.ps1               ← Unified installer (Windows PowerShell)
├── install.bat               ← Batch wrapper for PowerShell
│
├── assets/
│   ├── kzb_header.webp       ← Project header image
│   └── .gitkeep
│
├── botter/                   ← Vision Agent (Rust, ~8400 LOC, farming AI)
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
│   │   │   └── game_manager.rs ← GameManager (7-phase state machine)
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
├── maphack/                  ← Map Helper (Rust, ~3000 LOC, memory-based map reader)
│   ├── Cargo.toml            ← Rust project config
│   ├── Cargo.lock            ← Dependency lock file
│   ├── offsets.json.example  ← Example memory offsets file
│   ├── src/
│   │   ├── main.rs           ← Entry point, map memory reader
│   │   ├── discovery.rs      ← Process/window discovery
│   │   ├── host_registry.rs  ← Native messaging host registry helpers
│   │   ├── mapgen.rs         ← Map generation / tile assembly
│   │   ├── memory.rs         ← Memory reading interface
│   │   ├── offsets.rs        ← D2R memory structure offsets
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
│       ├── popup.html        ← Control panel UI (1521 LOC, 11 tabs, 503 settings, 77 kolbot scripts)
│       ├── popup.js          ← Control panel logic (372 LOC)
│       ├── popup.css         ← Control panel dark theme (615 LOC)
│       ├── map_content.js    ← Content script (map overlay injection, 260 LOC)
│       └── kzb_header.webp   ← Extension header image
│
└── kolbot/                   ← Classic D2 Bot (D2BS JavaScript reference)
    ├── D2Bot.exe             ← Manager executable
    ├── setup.bat             ← Setup batch wrapper
    ├── update.bat            ← Update batch script
    ├── .gitignore            ← Git exclusions
    ├── .gitmodules           ← Git submodule definitions
    ├── d2bs/
    │   ├── D2BS.dll          ← D2BS engine
    │   ├── HISTORY.txt       ← D2BS version history
    │   ├── LICENSE.rtf       ← License (RTF)
    │   ├── LICENSE.txt       ← License (text)
    │   └── api.html          ← API documentation (TiddlyWiki)
    │
    └── +setup/               ← Setup/installation scripts
        ├── d2bs.ini          ← D2BS configuration
        ├── setup.ps1         ← PowerShell installer
        └── starter/          ← Starter config templates
            ├── AdvancedConfig.js
            └── StarterConfig.js
```

---

## Key Files & Their Purpose

### Core Entry Points
- **`botter/src/main.rs`** -- Vision agent main loop
  - Argument parsing (config path selection)
  - DXGI capture initialization
  - Frame loop (25 Hz tick)
  - Native messaging host connection
  - Signal handling (graceful shutdown)

- **`maphack/src/main.rs`** -- Map helper main loop
  - D2R process discovery and memory reading
  - Map data parsing and generation
  - Native messaging host connection

- **`extension/chrome_extension/background.js`** (375 LOC) -- Chrome service worker
  - Manages two native messaging hosts
  - Bridges Chrome UI <-> Agent communication
  - Stats caching and event handling

### Decision & Logic
- **`botter/src/decision/engine.rs`**
  - Priority-based decision system
  - Survival checks (chicken, potions, TP)
  - Combat logic (dodge, static field, attack slots)
  - Attack target derivation (Boss/Champion/Normal/Immune)
  - Humanization (delays, variance, missed clicks)

- **`botter/src/decision/game_manager.rs`**
  - 7-phase state machine (OutOfGame -> Farming -> Exit)
  - Town automation (NPC sequences)
  - Game lifecycle (exit, inter-game delays)
  - Per-act NPC coordinates

### Vision & Capture
- **`botter/src/vision/capture.rs`**
  - Frame extraction from DXGI screenshot
  - Enemy detection (nearest, health %, type)
  - Loot detection (item quality, position)
  - Buff/debuff detection (visual indicators)
  - Merc HP, belt potions, inventory status

- **`botter/src/vision/shard_buffer.rs`**
  - Lock-free 16-shard FrameState buffer
  - Producer (capture thread) -> Consumer (decision thread)
  - ABA-protected concurrent reads

### Configuration
- **`botter/src/config/mod.rs`**
  - AgentConfig struct with 18 sections
  - Serde YAML serialization/deserialization
  - serde(default) for backward-compatibility
  - 8 pre-configured character YAMLs

### Stealth & Input
- **`botter/src/stealth/thread_input.rs`**
  - Thread-rotated 4-worker input pool
  - Per-thread jitter on SendInput calls
  - Round-robin dispatch

- **`botter/src/stealth/capture_timing.rs`**
  - 25 Hz frame capture timing
  - Skip/burst mode for dynamic frame rate
  - Timing jitter

- **`botter/src/stealth/process_identity.rs`**
  - PEB disguise (Windows, reports as NetworkService)
  - Command-line spoofing

- **`botter/src/stealth/syscall_cadence.rs`**
  - Decoy syscall injection
  - Breaks statistical fingerprinting

- **`botter/src/stealth/handle_table.rs`**
  - Pseudo-handle obfuscation

- **`botter/src/input/mod.rs`**
  - Input trait definition and types

- **`botter/src/input/simulator.rs`**
  - Simulation stubs for Linux-based testing/development

### Native Messaging
- **`botter/src/native_messaging/mod.rs`**
  - Chrome native messaging protocol (4-byte LE length + JSON)
  - Commands: pause, resume, get_stats, update_config, shutdown
  - Stats struct (SharedAgentStats with atomics)

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

### Testing
- **`botter/tests/stress.rs`**
  - 8 stress tests
  - 10s sustained loops, lock-free buffer stress
  - Thread pool throughput testing

- **`botter/benches/shard_bench.rs`**
  - Shard buffer performance benchmarks

- **`maphack/tests/protocol_test.py`**
  - Protocol integration tests (Python)

- **`maphack/tests/verify.py`**
  - Maphack verification tests (Python)

- **Unit tests** (scattered throughout src/)
  - 85 library tests
  - 99 binary integration tests
  - 8 stress tests
  - 192 total

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
cargo test                          # All tests (192)
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

| Component | Language | LOC |
|-----------|----------|-----|
| botter | Rust | ~8,400 |
| maphack | Rust | ~3,000 |
| **Total Rust** | | **~11,400** |
| extension | JS/CSS/HTML | ~3,100 |

| Test Suite | Count |
|------------|-------|
| Library unit tests | 85 |
| Binary integration tests | 99 |
| Stress tests | 8 |
| **Total** | **192** |

---

## Development Workflow

1. **Make changes** to Rust code in `botter/src/` or `maphack/src/`
2. **Run tests**: `cargo test` (all 192 tests should pass)
3. **Check lints**: `cargo clippy`
4. **Format code**: `cargo fmt`
5. **Commit**: `git commit -m "Clear message"`

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

- [ ] All tests pass (192/192)
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

- **D2R Memory Offsets**: See `maphack/src/offsets.rs` and `maphack/offsets.json.example`
- **Kolbot Docs**: `kolbot/d2bs/api.html`
- **D2 Forums**: Community reverse-engineering threads
- **Chrome Native Messaging**: [Google Docs](https://developer.chrome.com/docs/extensions/mv3/nativeMessaging/)
