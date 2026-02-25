# OpenClaw Domain Knowledge — D2R Farming Agent

> This document describes the game domain, observable signals, action space,
> and reward structure that an RL system (OpenClaw) needs to learn from
> the KZB training logs (`session_*.jsonl`).

---

## 1. Game Structure

### Acts & Town Hubs
D2R has 5 acts. Each act has a town (safe zone) where the agent heals,
shops, stashes loot, and repairs gear. Town NPCs are at fixed screen
positions (base 800x600, auto-scaled to actual resolution).

| Act | Town           | Key NPCs                                          |
|-----|----------------|----------------------------------------------------|
| 1   | Rogue Encampment | Akara (heal), Charsi (repair), Kashya (merc)     |
| 2   | Lut Gholein    | Fara (heal/repair), Drognan (potions), Greiz (merc)|
| 3   | Kurast Docks   | Ormus (heal), Hratli (repair), Asheara (merc)     |
| 4   | Pandemonium    | Jamella (heal), Halbu (repair), Tyrael (merc)      |
| 5   | Harrogath      | Malah (heal), Larzuk (repair), Qual-Kehk (merc)   |

### Run Cycle
A farming session is a loop:
1. Create or join game
2. Execute farm sequence (e.g. Mephisto → Pit → Baal)
3. Return to town between areas (heal, repair, stash)
4. Exit game, repeat

Runs typically last 2-8 minutes depending on area difficulty and clear speed.

---

## 2. Observable State (FrameState)

Every tick (~25 Hz capture, ~385 Hz processing), the vision pipeline
produces a `FrameState` snapshot. These are the signals OpenClaw can learn from:

| Field              | Type    | Range       | Meaning                              |
|--------------------|---------|-------------|--------------------------------------|
| `hp_pct`           | u8      | 0–100       | Player health percentage             |
| `mana_pct`         | u8      | 0–100       | Player mana percentage               |
| `enemy_count`      | u8      | 0–255       | Visible enemies with HP bars         |
| `in_combat`        | bool    | —           | True if enemy_count > 0              |
| `in_town`          | bool    | —           | True if in town (safe zone)          |
| `loot_label_count` | u8      | 0–255       | Visible item labels on ground        |
| `merc_alive`       | bool    | —           | Mercenary alive status               |
| `motion_magnitude` | f32     | 0.0+        | Character movement speed proxy       |
| `tick`             | u64     | monotonic   | Frame counter since session start    |
| `tick_phase_ms`    | u16     | 0–40        | Time within current tick (jitter)    |
| `phase_confidence` | f32     | 0.0–1.0     | Vision pipeline confidence in state  |

### Derived Signals (not in FrameState, inferred from sequences)
- **Time in combat** — consecutive ticks where `in_combat == true`
- **HP trend** — rising/falling hp_pct over last N ticks
- **Kill rate** — enemy_count delta over time
- **Loot density** — loot_label_count peaks after combat ends
- **Death** — hp_pct drops to 0 (chicken should prevent this)

---

## 3. Action Space

The decision engine selects one action per tick from this set:

| Action          | Parameters                | When Used                         |
|-----------------|---------------------------|-----------------------------------|
| `drink_potion`  | belt_slot (0-3)           | HP/mana below threshold           |
| `cast_skill`    | key, screen_x, screen_y   | Combat: attack enemies            |
| `pickup_loot`   | screen_x, screen_y        | Post-combat: collect items        |
| `move_to`       | screen_x, screen_y        | Navigation: walk/teleport         |
| `town_portal`   | —                         | Emergency retreat or town visit   |
| `chicken_quit`  | —                         | HP critically low, exit game      |
| `recast_buff`   | key                       | Maintain active buffs             |
| `dodge`         | screen_x, screen_y        | Evade dangerous packs             |
| `switch_weapon` | —                         | MF swap before kill / CTA buff    |
| `click`         | screen_x, screen_y        | NPC interaction, UI elements      |
| `take_break`    | duration_secs             | Session humanization              |
| `idle_pause`    | duration_ms               | Micro-pause between actions       |
| `wait`          | —                         | No action needed this tick        |

### Priority System
Each action has a priority (0 = highest):
- **P0**: `chicken_quit` — survival override
- **P1**: `drink_potion` — stay alive
- **P2**: `cast_skill`, `dodge` — combat
- **P3**: `pickup_loot` — loot collection
- **P4**: `move_to`, `click` — navigation
- **P5**: `recast_buff`, `switch_weapon` — maintenance
- **P6+**: `idle_pause`, `take_break`, `wait` — downtime

---

## 4. Decision Tiers (QuadCache Lanes)

The decision engine uses a 4-lane QuadCache for O(1) decision dispatch:

| Lane | Name       | Frequency        | What It Checks                        |
|------|------------|------------------|---------------------------------------|
| 1    | Survival   | Every frame      | HP/mana thresholds, chicken, potions  |
| 2    | Combat     | Every 3 frames   | Enemy targeting, skill selection      |
| 3    | Loot       | Every 5 frames   | Item labels, pickup priority          |
| 4    | Strategic  | Every 10 frames  | Buff timers, town triggers, pathing   |

### Lane 1: Survival (Critical Path)
This lane is the most important for RL to understand — it's the difference
between life and death.

**Key thresholds (from SurvivalConfig):**
- `chicken_hp_pct`: Exit game if HP falls below this (default: 30%)
- `hp_potion_pct`: Drink HP potion if below this (default: 75%)
- `hp_rejuv_pct`: Drink rejuvenation if below this (default: 40%)
- `tp_retreat_pct`: Cast Town Portal if below this (default: 35%)

**Learning opportunity**: Optimal threshold values vary by:
- Character class (Sorceress = squishy, Barbarian = tanky)
- Gear quality (early game = aggressive chicken, late game = relax)
- Area danger (Baal waves = tighter thresholds)

### Lane 2: Combat
**Skill selection depends on target type:**
- Boss → `boss_primary` / `boss_untimed` attack slots
- Champion → `mob_primary` with priority
- Normal → `mob_primary` / `mob_untimed`
- Immune → `immune_primary` / `immune_untimed` fallback

**Learning opportunity**: Which skill to use when, cast frequency,
positioning relative to enemies.

### Lane 3: Loot
Only activates when `enemy_count <= 2` (safe to loot).

**Pickup priority** (hardcoded, could be learned):
1. Unique/Set items (always pick)
2. Runes (always pick)
3. Rare rings/amulets (conditional)
4. Gold above threshold
5. Magic charms

### Lane 4: Strategic
- Town visit triggers (belt low, inventory full, merc dead)
- Buff recasting timers
- Game time management
- Area transition decisions

---

## 5. Reward Signals

OpenClaw should derive rewards from the JSONL training logs. Suggested
reward decomposition:

### Positive Rewards
| Signal                  | Weight | Source                            |
|-------------------------|--------|-----------------------------------|
| Loot picked up          | +1.0   | `action_type == "pickup_loot"`    |
| High-value loot         | +5.0   | Unique/Set/Rune detection         |
| Run completed           | +2.0   | Game exit without death           |
| Fast clear time         | +0.5   | Inverse of ticks-per-run          |
| Efficient potion use    | +0.2   | Potion used AND hp_pct recovered  |

### Negative Rewards
| Signal                  | Weight | Source                            |
|-------------------------|--------|-----------------------------------|
| Death (hp_pct → 0)      | -10.0  | Missing from logs = session crash |
| Chicken quit             | -2.0   | `action_type == "chicken_quit"`   |
| Wasted potion           | -0.5   | Potion at high hp_pct (>90%)      |
| Idle in combat          | -0.3   | `action_type == "wait"` while `in_combat` |
| Long town visit         | -0.1   | Extended `in_town` sequences      |

### Meta-Rewards (per-run)
- **Runs per hour** — efficiency metric
- **Deaths per hour** — survival metric
- **Loot value per hour** — ultimate optimization target
- **Chicken rate** — too high = thresholds too tight, too low = dying

---

## 6. Training Log Format

Each line in `session_YYYYMMDD_HHMMSS.jsonl` is a JSON object:

```json
{
  "timestamp": "2025-02-24T12:34:56.789Z",
  "tick": 42000,
  "tick_phase_ms": 450,
  "phase_confidence": 0.95,
  "state": {
    "hp_pct": 75,
    "mana_pct": 60,
    "enemy_count": 3,
    "in_combat": true,
    "in_town": false,
    "loot_labels": 2,
    "merc_alive": true,
    "motion_magnitude": 8.5
  },
  "action_type": "cast_skill",
  "action_detail": { "key": "f", "x": 640, "y": 360 },
  "delay_ms": 200,
  "priority": 2,
  "reason": "high_threat"
}
```

### Key Fields for RL
- **state**: Full observation at decision time
- **action_type + action_detail**: The action taken
- **delay_ms**: Humanized delay before execution (includes reaction time)
- **priority**: Which decision tier produced this action
- **reason**: Human-readable explanation from decision engine

### Run Boundaries
Detect run boundaries by looking for:
- `in_town` transitions (entering/leaving town)
- `chicken_quit` actions (forced game exit)
- Large tick gaps (new game session)
- Town portal sequences (mid-run retreat)

---

## 7. Character Classes & Builds

Each class has fundamentally different play patterns:

| Class       | Archetype   | Key Mechanic           | Survival Style      |
|-------------|-------------|------------------------|---------------------|
| Sorceress   | Ranged DPS  | Teleport, AoE spells   | Squishy, run away   |
| Paladin     | Melee tank  | Auras, Smite/Hammer    | Tanky, facetank      |
| Necromancer | Summoner    | Army of skeletons      | Hide behind minions  |
| Amazon      | Ranged      | Javelin/Bow combos     | Kite and shoot       |
| Barbarian   | Melee       | Whirlwind, Find Item   | High HP, leech       |
| Druid       | Hybrid      | Summons + elemental    | Mixed survivability  |
| Assassin    | Melee/trap  | Traps, Burst of Speed  | Fade for resistance  |

### Build Variants (examples)
- **Blizzard Sorceress**: Pure cold AoE, teleport between packs, moat trick bosses
- **Hammerdin**: Blessed Hammer spam, high survivability, versatile
- **Summon Necro**: Raise army, Corpse Explosion for clear, safe but slow
- **Javazon**: Lightning Fury for packs, Charged Strike for bosses

---

## 8. Vision Calibration

The vision pipeline uses hardcoded color thresholds for detection:
- **HP orb**: Red channel intensity in circular region
- **Mana orb**: Blue channel intensity in circular region
- **Enemy bars**: Red horizontal bars above enemies
- **Loot labels**: White/colored text on dark background
- **Town detection**: Specific color patterns in known screen regions

### Calibration Flag
When `calibration.enabled` is true in config, the agent enters a
special mode on startup:
1. Samples screen colors at known orb positions
2. Logs detected vs expected color ranges
3. Reports confidence in threshold accuracy
4. User reviews output and adjusts if needed

This is a **manual diagnostic tool**, not auto-correction. The user
triggers it, reviews the output, and decides whether to adjust
display settings (brightness, gamma, etc.) in D2R.

---

## 9. What OpenClaw Should Learn

### Phase 1: Imitation Learning (Watch Logs)
- Reproduce the existing decision engine's behavior
- Learn the mapping: state → action
- Validate against held-out log data

### Phase 2: Threshold Optimization
- Tune survival thresholds per class/build/area
- Optimize potion usage (minimize waste, maximize survival)
- Learn when to chicken vs when to fight through

### Phase 3: Strategic Decisions
- Optimal farm sequence ordering
- When to skip immune packs vs fight through
- Town visit timing (batch errands efficiently)
- Buff management (recast timing optimization)

### Phase 4: Skill Selection
- Which attack skill for which monster composition
- Positioning optimization (teleport placement)
- AoE vs single-target decision boundaries
- Dodge timing and direction

---

## 10. Glossary

| Term          | Meaning                                                    |
|---------------|-------------------------------------------------------------|
| Chicken       | Emergency quit game when HP is critically low               |
| MF            | Magic Find — gear stat that increases rare item drop chance |
| TP            | Town Portal — creates a portal back to town                 |
| Merc          | Mercenary — hired NPC companion that fights alongside you   |
| Belt slot     | One of 4 potion columns in the belt UI (0–3)               |
| Rejuv         | Rejuvenation potion — instant full heal (HP + mana)         |
| CTA           | Call to Arms — weapon runeword that gives Battle Orders buff|
| Moat trick    | Positioning exploit where boss can't reach you across water |
| QuadCache     | 4-lane O(1) decision accelerator in the vision agent        |
| DXGI          | DirectX Graphics Infrastructure — screen capture API        |
| Pickit        | Item filter rules (which items to pick up)                  |
| NIP           | Notation for item properties used in pickit filters         |
