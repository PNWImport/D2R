# KZB — Integration Memory

Living doc. Update this whenever something gets wired, fixed, or discovered broken.
Treat it as ground truth for "what actually works right now."

---

## Architecture Overview

```
Chrome Extension (background.js)
    │
    ├── Native Messaging → vision/chrome_helper.exe    (vision + input agent)
    └── Native Messaging → overlay/chrome_map_helper.exe  (maphack + Win32 overlay)

chrome_helper.exe
    ├── DXGI capture thread (25 Hz)  →  CapturePipeline  →  ShardedFrameBuffer
    ├── Decision loop (25 Hz)        →  GameManager  →  ProgressionEngine
    └── ThreadRotatedInput           →  actual Win32 input (keypress / click)

chrome_map_helper.exe
    ├── ReadProcessMemory → D2R.exe  (area_id, player_x/y, map_seed, difficulty)
    ├── mapgen backend (d2-map.exe)  →  MapData + MapPOI (exits, WP, shrines)
    └── Win32 layered overlay window →  draw_debug_state (HP/MP bars, enemy crosshair)
```

**The Chrome extension is the broker between both native hosts.**
It reads from one and can write to both. Currently it only relays:
- vision agent → map host (via `update_debug_state` every 100ms)
- map host → tabs/content scripts (via `MAP_UPDATE` broadcast)
- map host → **vision agent** = **NOT WIRED** ← THIS IS THE GAP

---

## Component Status

### Vision Pipeline (`vision/src/vision/capture.rs`)

| Detection | Status | Notes |
|---|---|---|
| HP orb % | ✅ Working | Fixed 2026-02 — cx was 5.3% from edge, now 16.6% |
| Mana orb % | ✅ Working | Same fix |
| at_menu | ✅ Working | Derived from hp_pct<=5 && mana_pct<=5 |
| Enemy count + position | ✅ Working | Scans for red HP bars |
| Boss/champion detection | ✅ Working | Bar width heuristic |
| Immune detection | ✅ Working | Cyan text color scan |
| Loot label detection | ✅ Working | 6 quality colors |
| Town detection | ✅ Working | Stone floor color heuristic |
| Merc alive/HP | ✅ Working | Portrait bar at top-left |
| Area name banner | ⚠️ Stub | Returns `"_banner_detected"` — no OCR |
| Quest complete banner | ✅ Working | Gold pixel density check |
| XP bar % | ✅ Working | Yellow strip at screen bottom |
| Belt potion columns | ✅ Working | Brightness scan at belt area |
| loading_screen | ❌ Not implemented | Always false |
| inventory_full | ❌ Not implemented | Always false |
| char_level | ❌ Not implemented | Always 0 |

### Phase Machine (`vision/src/decision/game_manager.rs`)

| Phase | Status | Notes |
|---|---|---|
| OutOfGame → TownPrep | ✅ Working | Requires 3 stable in_town frames |
| OOG menu clicking | ✅ Working | Only clicks when at_menu=true |
| OOG mid-dungeon fallback | ✅ Working | Transitions to Farming after 2s |
| TownPrep → Farming | ✅ Working | After heal/sell/ID/restock sequence |
| Farming → Returning | ✅ Working | On run complete or chicken |
| Returning → TownPrep | ✅ Working | On in_town stable |
| Any → OutOfGame | ✅ Working | On at_menu=true (game crash/exit) |

### Progression Engine (`vision/src/decision/progression.rs`)

| Feature | Status | Notes |
|---|---|---|
| Quest state persistence | ✅ Working | JSON at `C:\ProgramData\DisplayCalibration\quest_state.json` |
| Act 1-5 script queue | ✅ Working | Full sequence defined |
| shouldRun / skipIf logic | ✅ Working | Level thresholds, quest gates |
| Area name constants | ✅ Defined | All areas defined as string constants |
| Area inference from quest state | ❌ Not implemented | Needs `infer_current_area()` |
| Script executor navigation | ⚠️ Partial | Has move/attack but no pathfinding |

### Maphack / Map Host (`overlay/`)

| Feature | Status | Notes |
|---|---|---|
| D2R memory reading | ✅ Working | area_id, player_x/y, map_seed, difficulty |
| area_id → area_name | ✅ Working | 136 area IDs mapped in offsets.rs |
| Map generation (POIs/collision) | ✅ Working | Requires d2-map.exe backend |
| Exit/waypoint positions | ✅ Available | MapPOI with game coordinates |
| Win32 debug overlay | ✅ Working | HP/MP bars now near actual orbs |
| D2R offsets | ⚠️ Stale | Static offsets from D2R 2.x — game is on 3.x |
| Sig-scan (auto-find) | ✅ Hardened | 6 patterns, uniqueness enforcement, validate() gate |
| Auto-discovery fallback | ✅ Working | Heuristic scan when sig-scan fails |
| Demo mode | ✅ Default ON | Returns synthetic map data; toggle via SetDemoMode |
| GetOffsets command | ✅ Fixed | Returns actual resolved addresses (was returning stale defaults) |

### Chrome Extension (`extension/chrome_extension/background.js`)

| Data Flow | Status | Notes |
|---|---|---|
| Extension → vision agent (config/pause/resume) | ✅ Working | |
| Extension → map host (read_state poll 500ms) | ✅ Working | When map active |
| Vision agent → extension (frame_state 100ms) | ✅ Working | |
| Extension → map host (update_debug_state) | ✅ Working | Relays vision frame state |
| **Map host → extension (game_state + POIs)** | ✅ Working | Handled in handleMapMessage |
| **Extension → vision agent (map state + POIs)** | ❌ **NOT WIRED** | **← THE GAP** |

---

## The Gap: Map Data → Vision Agent

The map host already delivers every tick:
```js
msg.game_state = { area_id, area_name, player_x, player_y, map_seed, difficulty, in_game, is_town }
msg.map = { pois: [ { x, y, poi_type, label, target_area } ] }
```

The vision agent's game_manager receives NONE of this. It navigates blind.

### ❌ REJECTED: Map host → vision agent relay

Forwarding `ReadProcessMemory` data into the vision agent defeats the whole point.
The vision agent uses DXGI screen capture specifically to avoid opening handles on
D2R.exe. Feeding it memory-read data doesn't make it safer — it just adds the
detection risk of the map host on top.

**The map host (`chrome_map_helper.exe`) is OPTIONAL and RISKY:**
- Opens `OpenProcess(D2R.exe)`
- Reads `ReadProcessMemory` for offsets
- Detectable by any AC that scans handle tables
- Useful for the visual overlay canvas (minimap rendering in Chrome tab)
- Should NOT be the navigation data source for the vision agent

### ✅ Solution: Minimap visual detection (implemented)

D2R's minimap (top-right HUD) shows exit chevrons as bright gold/yellow and
waypoints as cyan. The vision pipeline can detect these with pixel color analysis —
no memory reads, just the same DXGI frame that everything else uses.

**Flow:**
```
DXGI frame → detect_minimap_markers() → minimap_exit_screen_x/y
                                       → minimap_wp_screen_x/y
game_manager.navigate_toward_minimap_exit(frame) → screen teleport target
```

**Color signatures:**
- Exit chevron : R>210, G>160, B<90, R-B>140 (gold/yellow)
- Waypoint dot : B>180, G>130, R<130, B-R>80 (cyan/teal)
- Minimap region: circle at ~89.8% x, ~15.9% y, radius ~7.4% of frame width

**Navigation math:**
```
mm_cx, mm_cy = minimap center (player position in minimap space)
dx = exit_marker_x - mm_cx   (direction to exit in minimap pixels)
dy = exit_marker_y - mm_cy
target_x = player_screen_x + dx * 20   (scale 20 = ~25% of full minimap scale)
target_y = player_screen_y + dy * 20
→ Teleport there. Repeat until loading screen triggers.
```

Scale calibration: minimap radius ~95px covers ~240 tiles → 0.4px/tile.
Screen ~40 tiles at 1280px → 32px/tile. Full scale = 80. Step scale = 20.
TODO: calibrate actual scale in-game, may need tuning.

### What needs to happen (in order):

1. **`background.js`** — in `handleMapMessage("state")`, forward map state to vision agent:
   ```js
   agentPort.postMessage({ cmd: "update_map_state", game_state: msg.game_state, pois: msg.map?.pois ?? [] })
   ```

2. **`vision/src/native_messaging/mod.rs`** — add `"update_map_state"` command handler
   → sends `AgentCommand::UpdateMapState { area_id, area_name, player_x, player_y, pois }`

3. **`vision/src/decision/game_manager.rs`** — store `MapState`, use `area_name` from it
   (replaces the `"_banner_detected"` placeholder)

4. **`vision/src/decision/game_manager.rs`** — navigation: use POI coordinates
   - Convert game-coords → screen-coords via isometric projection
   - Use `Teleport` action targeting exit POI position

### Isometric Coordinate Conversion (D2R)

D2R uses an isometric projection. Converting game (world) coords to screen:
```
// At 1280x720, player is at screen center (640, 360)
// D2R isometric: each world tile = ~32px wide, ~16px tall on screen
dx_world = target_x - player_x
dy_world = target_y - player_y
screen_x = 640 + (dx_world - dy_world) * 16
screen_y = 360 + (dx_world + dy_world) * 8
```
The scale factors (16, 8) depend on zoom level. At default zoom these are approximate.
TODO: Calibrate exact scale values from in-game measurement.

---

## Andariel Navigation (Act 1 Boss)

The problem: D2R maps are randomly generated each game.
The bot cannot hardcode "go to (X,Y) for the stairs."

### Required flow:
1. Map host reads: `area_id=Jail Level 1 (17)`, `seed=0xABCD`, `difficulty=0`
2. Map host calls d2-map.exe backend → gets `MapData` with `MapPOI` list
3. POIs include: `{ type: Staircase, x: 112, y: 87, target_area: 18 }` (stairs to Jail 2)
4. Background.js relays POIs → vision agent
5. Game manager converts (112, 87) to screen coords → teleports there
6. Repeat for Jail 2→3, then Inner Cloister, Catacombs 1→4
7. In Catacombs 4: no exit, just kill everything → Andariel spawns at center

### Current state:
- Steps 1-2: ✅ Map host can do this
- Step 3: ✅ MapPOI types exist
- Step 4: ❌ Not wired (the gap above)
- Step 5: ❌ No isometric coord conversion
- Step 6-7: ❌ No multi-level navigation logic

### Waypoint shortcut:
Once WP is unlocked for Jail Level 1, the bot can skip Outer Cloister entirely.
The script_executor should use WP when available (quest_state tracks WP unlocks).

---

## D2R Offset Status

**WARNING**: Static offsets in `overlay/src/offsets.rs` are from D2R 2.x era.
Current game version: 3.x ("Reign of the Warlock" patch series).

### Resolution order (overlay/src/offsets.rs + memory.rs):

1. **Sig-scan** (automatic on attach) — 6 byte patterns scanned over D2R.exe ~34 MB image.
   Uniqueness enforced: each pattern must match exactly 1 location or it's rejected.
   Fills `player_hash_table` and `ui_settings` with resolved addresses.
2. **offsets.json override** — `C:\ProgramData\DisplayCalibration\offsets.json` can provide
   manual overrides. Supports `disable_sigs: ["PatternName"]` to skip broken patterns.
3. **Auto-discovery heuristic** — `discovery.rs` runs when sig-scan fails. Broader search
   using structural heuristics (pointer chains, alignment checks).
4. **validate() gate** — after all resolution, `offsets.validate()` checks that both
   `player_hash_table` and `ui_settings` are non-zero. If either is 0, memory reading
   is blocked and an error is logged.

### Demo mode (default: ON):

Demo mode starts enabled so the Chrome overlay works immediately for visual testing.
Synthetic map data (cellular automata generator) is returned instead of RPM reads.
Toggle via extension popup or `SetDemoMode { enabled: false }` from background.js.
Use `get_offsets` command to check `sig_scan_complete` and `offsets_valid` without
leaving demo mode.

### Community sources for current offsets:
- D2RMH GitHub issues
- MapAssist community fork patch notes
- Slashdiablo/d2r modding Discord

---

## 1280×720 Layout Status

All coordinate systems have been migrated or confirmed for 1280×720:

| System | Approach | Status |
|---|---|---|
| HP/Mana orbs | Exact measured positions per resolution in `OrbLayout::for_resolution()` | ✅ Correct |
| NPC positions (game_manager.rs) | Base coords at 800×600 → `scale_npc_pos(pos, fw, fh)` at runtime | ✅ Correct |
| NPC positions (script_executor.rs) | Base coords at 800×600 → `sx()/sy()` scaling at runtime | ✅ Correct |
| WP panel coordinates | Base 800×600 → `sx()/sy()` scaling | ✅ Correct |
| Minimap detection | Fractional positions (89.8% x, 15.9% y) — resolution-independent | ✅ Correct |
| Chrome popup (popup mode) | Fixed 780px width (fits Chrome popup at any resolution) | ✅ OK |
| Chrome popup (tab mode) | 900-1400px responsive width | ✅ OK at 1280 |
| Map overlay canvas | Fixed 300×300 (minimap in corner) | ✅ OK |
| Char center default | (640, 360) = 1280×720 center | ✅ Correct |

**Design decision**: NPC coordinates are stored at 800×600 base (from Kolbot Town.js)
and scaled proportionally. This is correct because D2R maintains proportional viewport
scaling. Direct 1280×720 hardcoding was considered but rejected — the scaling approach
handles all resolutions via the same code path.

### Build & test verification (2026-02-25):
- `cargo check` — both crates compile clean (0 warnings)
- `cargo test` — **310 tests pass** (158 unit + 8 stress × 2 runs, 0 failures)
- Compilation time: ~8s (vision), ~3s (overlay)

---

## Changelog

| Date | Change |
|---|---|
| 2026-02-25 | Fix orb coordinates: hp_cx was 5.3% from edge (wrong), now 16.6% |
| 2026-02-25 | Fix overlay HP/MP draw position: was (10,10) top-left, now near orbs |
| 2026-02-25 | Add at_menu detection via orb visibility (hp<=5 && mana<=5) |
| 2026-02-25 | Fix handle_oog: stop clicking center-screen when in-game HUD visible |
| 2026-02-25 | Fix phase transition: OutOfGame→Farming after 2s mid-dungeon |
| 2026-02-25 | Fix test_oog_to_town_transition: needs 3 calls for stability counter |
| 2026-02-25 | Fix install.ps1 PowerShell parentheses syntax errors |
| 2026-02-25 | Adjust all coords from 800x600 to 1280x720 baseline |
| 2026-02-25 | Wire maphack→vision relay then REVERTED: RPM data in vision defeats detection avoidance |
| 2026-02-25 | Add minimap exit/WP detection (pure DXGI pixel scan, gold/cyan centroids) |
| 2026-02-25 | Add navigate_toward_minimap_exit/waypoint — screen-space nav from marker offset |
| 2026-02-25 | Fix detect_minimap_markers: frame.data → frame.pixels, raw offset → stride-based |
| 2026-02-25 | Add minimap_visible field + detect_minimap_visible() (dark-background pixel sampling) |
| 2026-02-25 | Add ensure_minimap_open() — returns Tab-press Decision if minimap not in mini mode |
| 2026-02-25 | Guard marker scan behind minimap_visible — skip gold/cyan scan when map is off |
| 2026-02-25 | Enable demo mode by default — overlay works immediately for visual testing |
| 2026-02-25 | Harden sig-scan: uniqueness enforcement, disable_sigs, validate() gate |
| 2026-02-25 | Fix GetOffsets: was returning stale defaults, now exposes actual resolved addresses |
| 2026-02-25 | Verify 1280×720 layout: all coords scale correctly, 310/310 tests pass |
