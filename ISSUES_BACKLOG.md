# D2R Bot — Technical Backlog

## Known Gaps (Post-Install Check & Walkthrough)

### 🔴 CRITICAL — Vision Layer

#### Horadric Cube Mechanics (Cubing System)
- **Issue**: Cubing is stub-level — `InteractObject("Horadric Cube")` doesn't drive UI flow
- **Affected scripts**:
  - Cow Level (Wirt's Leg + TP Tome)
  - Travincal (Khalim's Eye + Heart + Brain + Flail → Will)
  - Horadric Staff assembly (Amulet + Shaft → Staff)
- **Complexity**: HIGH
  - Requires: open cube UI → detect grid → place items via OCR/detection → find Transmute button → click
  - Vision pipeline: item detection, grid boundary detection, button localization
- **Impact**: Can't run Cow Level, Travincal assembly, Act 2 staff quests in progression
- **PR estimate**: Medium (vision-heavy, low business priority)

### 🟡 MEDIUM — Progression

#### Waypoint Auto-Detection Not Fully Wired
- **Issue**: `on_waypoint_obtained()` handles all 30+ waypoint area names, but the *detector* that calls it is missing
- **Current state**: Waypoint flags are set manually in plans but never updated from visual detection
- **What's needed**: Vision pipeline to detect "waypoint obtained" visual cue (waypoint icon on minimap or banner)
- **Impact**: Waypoint cache never populates from actual gameplay; only works if pre-configured
- **PR estimate**: Small (config integration)

#### Tristram Cairn Stone Sequence
- **Issue**: Bot clicks 5 Cairn Stones individually but doesn't differentiate them (anonymous positional clicks)
- **Why it works**: Stone positions are spread around player, clicks cycle through all 5
- **Why it's fragile**: If stones don't activate in expected order, bot may miss one or click wrong position
- **Solution**: Detect active stone state (glow/visual change) before advancing to next
- **PR estimate**: Small (vision flag check)

### 🟢 LOW — Robustness

#### Stat Inflation from Legacy Action Types
- **Fixed in recent commit**: `jittered_click()` now uses `Action::Click` instead of `Action::PickupLoot`
- **Residual issue**: Other places in codebase still use semantic mismatches (check for any remaining)
- **Impact**: Stats (`loots_picked`, etc.) may be inflated
- **PR estimate**: Trivial (search/replace)

#### Process Identity Persistence Across Games
- **Feature**: Random `ChromeDisguise` selection per launch (done)
- **Enhancement**: Consider rotating identity between games (new process spawn, new disguise)
- **Current behavior**: Same disguise for entire agent lifetime (until process restart)
- **Stealth implication**: Repeated identity could be fingerprinted across game sessions
- **PR estimate**: Small (add timer/epoch counter to game_manager)

---

## Completed in This Session

✅ **Install.ps1 binary path fix** — `d2_vision_agent` → `kzb_vision_agent`
✅ **Diablo seal plan logic** — Remove premature `WaitForCue(QuestCompleteBanner)` before killing Diablo
✅ **Action::Click variant** — Proper semantic separation from `PickupLoot` and `MoveTo`
✅ **Random process identity** — ChromeDisguise rotation at startup
✅ **Waypoint mapping** — All 30+ waypoints now tracked in `on_waypoint_obtained()`
✅ **282/282 tests passing** — Full test suite clean

---

## Testing & Validation Needed

- [ ] Speedtest with quadcache live (farm run execution time baseline)
- [ ] Level 1 Act 1 walkthrough (progression + cache hit analysis)
- [ ] Cubing UI detection prototype (vision layer feasibility check)
- [ ] Waypoint visual detector integration
