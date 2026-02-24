#![allow(dead_code)]
// =============================================================================
// D2R Memory Offsets & Struct Layouts
// =============================================================================
//
// !! PATCH STATUS !!
// D2R is currently on v3.1.1 "Reign of the Warlock" (Feb 19, 2026).
// This is a MASSIVE update: new Warlock class, new areas, loot filter, etc.
// The binary has changed significantly from the 2.x era.
//
// STATIC OFFSETS BELOW ARE FROM 2.x ERA AND WILL NOT WORK ON 3.x.
// They are retained as documentation only.
//
// The CORRECT approach for 3.x:
//   1. Sig-scan patterns (most resilient - survive minor patches)
//   2. offsets.json config file (user-provided from CE/IDA/community)
//   3. Static offsets as absolute last resort (will fail on 3.x)
//
// STRUCT LAYOUTS (field offsets within structs) are generally stable
// across patches because the engine data structures don't change much.
// What DOES change is the STATIC BASE ADDRESS of the hash tables and
// global pointers. Sig-scan handles this.
//
// TO UPDATE FOR A NEW PATCH:
//   Option A: Run sig-scan (built-in, automatic on attach)
//   Option B: Place offsets.json next to the exe with:
//     { "player_hash_table": "0x2028E60", "ui_settings": "0x20AD5F0" }
//   Option C: CE/IDA dump new statics, update the consts below
//
// Sources:
//   - MapAssist (D2RLegit) - Helpers/Offsets.cs (GPL-3.0)
//   - D2RMH (soarqin) - d2r_process.cpp
//   - NTQV fork (joffreybesos/Bandit) - v1.8/v2.7+ offsets
//   - OwnedCore community research
//   - d2r-mapview (joffreybesos) - AHK offsets
// =============================================================================

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Static base offsets (PATCH-DEPENDENT — last validated ~2.7/2.8)
// These are FALLBACKS for sig-scan failure only.
// Sig-scan (below) is the correct approach for any new patch.
// Source: OwnedCore community + MapAssist + D2RMH
// ---------------------------------------------------------------------------

/// Player Unit Hash Table (128 buckets of linked UnitAny*)
/// Sig-scan SIG_UNIT_HASH_TABLE finds the live address; this is the fallback.
pub const PLAYER_UNIT_HASH_TABLE: u64 = 0x2028E60;

/// UI Settings base (menu state, automap toggle)
pub const UI_SETTINGS_BASE: u64 = 0x20AD5F0;

/// Expansion check (LoD vs Classic)
pub const EXPANSION_CHECK: u64 = 0x20AD3B0;

/// Roster data (party members)
pub const ROSTER_DATA: u64 = 0x20B1B78;

/// Game name string
pub const GAME_NAME_OFFSET: u64 = 0x20AD678;

// ---------------------------------------------------------------------------
// Sig-Scan Patterns (PATCH-RESILIENT)
// These byte patterns survive most patches because they match instruction
// sequences near the data, not absolute addresses.
// The wildcards (mask='?') match the RIP-relative offset bytes.
//
// Source: confirmed by both MapAssist (D2RLegit/MapAssist ProcessContext.cs)
//         and D2RMH (soarqin/D2RMH processdata.cpp) — identical patterns.
//
// Resolve modes:
//   RipRelative:  result = (scan_base + match + addr_offset + addr_size) + rel + extra
//   BaseRelative: result = scan_base + rel + extra  (val is RVA from module base)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveMode {
    /// Standard x64 RIP-relative addressing:
    /// result = (instruction_end) + rel + extra
    RipRelative,
    /// Value is an offset from the module base:
    /// result = scan_base + rel + extra
    BaseRelative,
}

#[derive(Debug, Clone)]
pub struct SigPattern {
    pub name: &'static str,
    pub pattern: &'static [u8],
    pub mask: &'static str,       // 'x' = match, '?' = wildcard
    pub addr_offset: i64,         // signed — byte offset to the i32 (can be negative)
    pub addr_size: usize,         // typically 4 (i32)
    pub extra_offset: i64,        // additional offset after resolution
    pub mode: ResolveMode,
}

/// Unit Hash Table: "48 03 C7 49 8B 8C C6"
///   ADD RAX, RDI / MOV RCX, [R14 + RAX*8 + disp32]
///   The disp32 at +7 is an RVA from module base.
/// Source: MapAssist + D2RMH search0 (validated 2.4 – 2.8+)
pub const SIG_UNIT_HASH_TABLE: SigPattern = SigPattern {
    name: "UnitHashTable",
    pattern: &[0x48, 0x03, 0xC7, 0x49, 0x8B, 0x8C, 0xC6],
    mask: "xxxxxxx",
    addr_offset: 7,
    addr_size: 4,
    extra_offset: 0,
    mode: ResolveMode::BaseRelative,
};

/// UI Settings / Menu Data: "48 89 45 B7 4C 8D 35 ?? ?? ?? ??"
///   LEA R14, [rip + disp32] — loads UI settings base
/// Source: MapAssist + D2RMH search1
pub const SIG_UI_SETTINGS: SigPattern = SigPattern {
    name: "UISettings",
    pattern: &[0x48, 0x89, 0x45, 0xB7, 0x4C, 0x8D, 0x35, 0x00, 0x00, 0x00, 0x00],
    mask: "xxxxxxx????",
    addr_offset: 7,
    addr_size: 4,
    extra_offset: 0,
    mode: ResolveMode::RipRelative,
};

/// Expansion check: "48 8B 05 ?? ?? ?? ?? 48 8B D9 F3 0F 10 50"
///   MOV RAX, [rip + disp32]
/// Source: MapAssist + D2RMH search2
pub const SIG_EXPANSION: SigPattern = SigPattern {
    name: "Expansion",
    pattern: &[0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00, 0x48, 0x8B, 0xD9, 0xF3, 0x0F, 0x10, 0x50],
    mask: "xxx????xxxxxxx",
    addr_offset: 3,
    addr_size: 4,
    extra_offset: 0,
    mode: ResolveMode::RipRelative,
};

/// Roster Data: "02 45 33 D2 4D 8B"
///   The rel32 is 3 bytes BEFORE the match (addr_offset = -3).
/// Source: MapAssist + D2RMH search3
pub const SIG_ROSTER_DATA: SigPattern = SigPattern {
    name: "RosterData",
    pattern: &[0x02, 0x45, 0x33, 0xD2, 0x4D, 0x8B],
    mask: "xxxxxx",
    addr_offset: -3,
    addr_size: 4,
    extra_offset: 0,
    mode: ResolveMode::RipRelative,
};

/// Game Info / Name: "44 88 25 ?? ?? ?? ?? 66 44 89 25"
///   The rel32 at +3 resolves to an address 0x121 bytes before the instruction.
/// Source: MapAssist + D2RMH search4
pub const SIG_GAME_INFO: SigPattern = SigPattern {
    name: "GameInfo",
    pattern: &[0x44, 0x88, 0x25, 0x00, 0x00, 0x00, 0x00, 0x66, 0x44, 0x89, 0x25],
    mask: "xxx????xxxx",
    addr_offset: 3,
    addr_size: 4,
    extra_offset: -0x128, // -(0x121 + 7)
    mode: ResolveMode::RipRelative,
};

/// Map Seed (direct): "41 8B F9 48 8D 0D ?? ?? ?? ??"
///   LEA RCX, [rip + disp32] — near the seed access code.
///   Resolves to address + 0xEA past the instruction end.
/// Source: D2RMH search5
pub const SIG_MAP_SEED: SigPattern = SigPattern {
    name: "MapSeed",
    pattern: &[0x41, 0x8B, 0xF9, 0x48, 0x8D, 0x0D, 0x00, 0x00, 0x00, 0x00],
    mask: "xxxxxx????",
    addr_offset: 6,
    addr_size: 4,
    extra_offset: 0xE0, // 0xEA - (6 + 4) accounts for instruction width
    mode: ResolveMode::RipRelative,
};

// ---------------------------------------------------------------------------
// D2R Struct Layouts (64-bit)
// These are INTRA-STRUCT offsets (field positions within a struct).
// Source: soarqin/D2RMH d2rdefs.h (verified against MapAssist)
// These are the authoritative D2R x64 offsets for the engine structures.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitAnyOffsets {
    pub unit_type: u64,        // +0x00  DWORD (0=Player,1=Monster,2=Object,3=Missile,4=Item,5=Tile)
    pub class_id: u64,         // +0x04  DWORD (txtFileNo: char class / monster ID)
    pub unit_id: u64,          // +0x08  DWORD (instance ID)
    pub mode: u64,             // +0x0C  DWORD (animation state)
    pub union_ptr: u64,        // +0x10  ptr → PlayerData/MonsterData/ObjectData/ItemData
    pub act_ptr: u64,          // +0x20  ptr → DrlgAct  (NOT 0x18 — 0x18 is unk0)
    pub seed: u64,             // +0x28  seed data
    pub path_ptr: u64,         // +0x38  ptr → DynamicPath / StaticPath
    pub stat_list_ptr: u64,    // +0x88  ptr → StatList
    pub inventory_ptr: u64,    // +0x90  ptr → Inventory
    pub skill_ptr: u64,        // +0xB8  ptr → Skill
    pub next_unit_ptr: u64,    // +0xE8  ptr → next UnitAny in hash chain
}

impl Default for UnitAnyOffsets {
    fn default() -> Self {
        Self {
            unit_type: 0x00, class_id: 0x04, unit_id: 0x08, mode: 0x0C,
            union_ptr: 0x10, act_ptr: 0x20, seed: 0x28, path_ptr: 0x38,
            stat_list_ptr: 0x88, inventory_ptr: 0x90, skill_ptr: 0xB8,
            next_unit_ptr: 0xE8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathOffsets {
    pub pos_x: u64,       // +0x02  WORD (DynamicPath.posX)
    pub pos_y: u64,       // +0x06  WORD (DynamicPath.posY)
    pub target_x: u64,    // +0x10  DWORD (DynamicPath.targetX)
    pub target_y: u64,    // +0x14  DWORD (DynamicPath.targetY)
    pub room_ptr: u64,    // +0x20  ptr → DrlgRoom1
}

impl Default for PathOffsets {
    fn default() -> Self {
        Self { pos_x: 0x02, pos_y: 0x06, target_x: 0x10, target_y: 0x14, room_ptr: 0x20 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActOffsets {
    pub act_misc_ptr: u64, // +0x78 ptr → ActMisc
    pub act_id: u64,       // +0x28 DWORD (0-4 for Act I-V)
}

impl Default for ActOffsets {
    fn default() -> Self { Self { act_misc_ptr: 0x78, act_id: 0x28 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActMiscOffsets {
    pub difficulty: u64,   // +0x830 DWORD (0=Norm, 1=NM, 2=Hell)
    pub map_seed: u64,     // +0x840 DWORD (the prize)
    pub level_first: u64,  // +0x868 ptr
}

impl Default for ActMiscOffsets {
    fn default() -> Self { Self { difficulty: 0x830, map_seed: 0x840, level_first: 0x868 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room1Offsets {
    pub room_next: u64,    // +0xB0  ptr → next DrlgRoom1
    pub room_ex_ptr: u64,  // +0x18  ptr → DrlgRoom2
    pub unit_first: u64,   // +0xA8  ptr → first UnitAny in room
    pub act_ptr: u64,      // +0x48  ptr → DrlgAct
}

impl Default for Room1Offsets {
    fn default() -> Self { Self { room_next: 0xB0, room_ex_ptr: 0x18, unit_first: 0xA8, act_ptr: 0x48 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room2Offsets {
    pub room2_next: u64,   // +0x48  ptr → next DrlgRoom2
    pub level_ptr: u64,    // +0x90  ptr → DrlgLevel
    pub pos_x: u64,        // +0x60  DWORD (tile X)
    pub pos_y: u64,        // +0x64  DWORD (tile Y)
    pub size_x: u64,       // +0x68  DWORD (tile width)
    pub size_y: u64,       // +0x6C  DWORD (tile height)
}

impl Default for Room2Offsets {
    fn default() -> Self {
        Self { room2_next: 0x48, level_ptr: 0x90, pos_x: 0x60, pos_y: 0x64, size_x: 0x68, size_y: 0x6C }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelOffsets {
    pub level_next: u64,   // +0x00  ptr → next DrlgLevel
    pub room2_first: u64,  // +0x10  ptr → first DrlgRoom2
    pub level_id: u64,     // +0x1F8 DWORD (area 1-136+) — D2RMH confirmed
}

impl Default for LevelOffsets {
    fn default() -> Self { Self { level_next: 0x00, room2_first: 0x10, level_id: 0x1F8 } }
}

// ---------------------------------------------------------------------------
// Area IDs (levels.txt)
// 3.x Reign of the Warlock may add new area IDs beyond 136.
// Existing IDs should remain the same.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum AreaId {
    None = 0,
    // Act 1
    RogueEncampment = 1, BloodMoor = 2, ColdPlains = 3, StonyField = 4,
    DarkWood = 5, BlackMarsh = 6, TamoeHighland = 7, DenOfEvil = 8,
    CaveLvl1 = 9, UndergroundPassLvl1 = 10, UndergroundPassLvl2 = 11,
    Pit = 12, CatacombsLvl4 = 37, MooMooFarm = 39,
    // Act 2
    LutGholein = 40, AncientTunnels = 65, ArcaneSanctuary = 74,
    DurancessOfHateLvl3 = 83,
    // Act 3
    KurastDocks = 75, Travincal = 84,
    // Act 4
    PandemoniumFortress = 103, RiverOfFlame = 107, ChaosSanctuary = 108,
    // Act 5
    Harrogath = 109, BloodyFoothills = 110,
    WorldstoneKeepLvl1 = 128, WorldstoneKeepLvl2 = 129, WorldstoneKeepLvl3 = 130,
    ThroneOfDestruction = 131, WorldstoneChamber = 132,
    // 3.x Reign of the Warlock — IDs TBD (likely 137+)
    // New Terror Zones, Colossal Ancients arena, Warlock areas
    // Will be added when community maps the new area IDs
}

impl AreaId {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::RogueEncampment, 2 => Self::BloodMoor, 3 => Self::ColdPlains,
            4 => Self::StonyField, 5 => Self::DarkWood, 6 => Self::BlackMarsh,
            7 => Self::TamoeHighland, 8 => Self::DenOfEvil,
            12 => Self::Pit, 37 => Self::CatacombsLvl4, 39 => Self::MooMooFarm,
            40 => Self::LutGholein, 65 => Self::AncientTunnels,
            74 => Self::ArcaneSanctuary, 75 => Self::KurastDocks,
            83 => Self::DurancessOfHateLvl3, 84 => Self::Travincal,
            103 => Self::PandemoniumFortress, 107 => Self::RiverOfFlame,
            108 => Self::ChaosSanctuary, 109 => Self::Harrogath,
            110 => Self::BloodyFoothills,
            128 => Self::WorldstoneKeepLvl1, 129 => Self::WorldstoneKeepLvl2,
            130 => Self::WorldstoneKeepLvl3, 131 => Self::ThroneOfDestruction,
            132 => Self::WorldstoneChamber,
            _ => Self::None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "Unknown",
            Self::RogueEncampment => "Rogue Encampment", Self::BloodMoor => "Blood Moor",
            Self::ColdPlains => "Cold Plains", Self::StonyField => "Stony Field",
            Self::DarkWood => "Dark Wood", Self::BlackMarsh => "Black Marsh",
            Self::TamoeHighland => "Tamoe Highland", Self::DenOfEvil => "Den of Evil",
            Self::CaveLvl1 => "Cave Level 1",
            Self::UndergroundPassLvl1 => "Underground Passage Lv1",
            Self::UndergroundPassLvl2 => "Underground Passage Lv2",
            Self::Pit => "The Pit", Self::CatacombsLvl4 => "Catacombs Level 4",
            Self::MooMooFarm => "Moo Moo Farm", Self::LutGholein => "Lut Gholein",
            Self::AncientTunnels => "Ancient Tunnels", Self::ArcaneSanctuary => "Arcane Sanctuary",
            Self::KurastDocks => "Kurast Docks", Self::DurancessOfHateLvl3 => "Durance of Hate Lv3",
            Self::PandemoniumFortress => "Pandemonium Fortress",
            Self::RiverOfFlame => "River of Flame", Self::ChaosSanctuary => "Chaos Sanctuary",
            Self::Harrogath => "Harrogath", Self::BloodyFoothills => "Bloody Foothills",
            Self::WorldstoneKeepLvl1 => "Worldstone Keep Lv1",
            Self::WorldstoneKeepLvl2 => "Worldstone Keep Lv2",
            Self::WorldstoneKeepLvl3 => "Worldstone Keep Lv3",
            Self::ThroneOfDestruction => "Throne of Destruction",
            Self::WorldstoneChamber => "Worldstone Chamber",
            Self::Travincal => "Travincal",
        }
    }

    pub fn is_town(&self) -> bool {
        matches!(self,
            Self::RogueEncampment | Self::LutGholein | Self::KurastDocks |
            Self::PandemoniumFortress | Self::Harrogath)
    }

    pub fn act(&self) -> u8 {
        match *self as u32 {
            1..=39 => 1, 40..=74 => 2, 75..=102 => 3,
            103..=108 => 4, 109..=136 => 5, _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime offsets bundle — supports JSON override for patch updates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct D2ROffsets {
    pub unit_any: UnitAnyOffsets,
    pub path: PathOffsets,
    pub act: ActOffsets,
    pub act_misc: ActMiscOffsets,
    pub room1: Room1Offsets,
    pub room2: Room2Offsets,
    pub level: LevelOffsets,
    pub player_hash_table: u64,
    pub ui_settings: u64,
}

impl Default for D2ROffsets {
    fn default() -> Self {
        Self {
            unit_any: UnitAnyOffsets::default(),
            path: PathOffsets::default(),
            act: ActOffsets::default(),
            act_misc: ActMiscOffsets::default(),
            room1: Room1Offsets::default(),
            room2: Room2Offsets::default(),
            level: LevelOffsets::default(),
            player_hash_table: PLAYER_UNIT_HASH_TABLE,
            ui_settings: UI_SETTINGS_BASE,
        }
    }
}

impl D2ROffsets {
    /// Try to load overrides from offsets.json next to the executable.
    /// Format: { "player_hash_table": "0x2028E60", "ui_settings": "0x20AD5F0",
    ///           "act_misc": { "map_seed": "0x840", "difficulty": "0x830" } }
    /// Only provided fields are overridden; missing fields keep defaults.
    pub fn load_overrides(&mut self) {
        let path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("offsets.json")));

        let path = match path {
            Some(p) if p.exists() => p,
            _ => return,
        };

        eprintln!("[map] Loading offset overrides from {:?}", path);

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => { eprintln!("[map] Failed to read offsets.json: {}", e); return; }
        };

        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => { eprintln!("[map] Failed to parse offsets.json: {}", e); return; }
        };

        // Helper to parse hex strings like "0x2028E60" or plain ints
        fn parse_hex(v: &serde_json::Value) -> Option<u64> {
            if let Some(s) = v.as_str() {
                u64::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
            } else {
                v.as_u64()
            }
        }

        // Override top-level statics
        if let Some(v) = parse_hex(&json["player_hash_table"]) {
            self.player_hash_table = v;
            eprintln!("[map]   player_hash_table = {:#X}", v);
        }
        if let Some(v) = parse_hex(&json["ui_settings"]) {
            self.ui_settings = v;
        }

        // Override struct field offsets
        if let Some(obj) = json.get("act_misc") {
            if let Some(v) = parse_hex(&obj["map_seed"]) { self.act_misc.map_seed = v; }
            if let Some(v) = parse_hex(&obj["difficulty"]) { self.act_misc.difficulty = v; }
        }
        if let Some(obj) = json.get("unit_any") {
            if let Some(v) = parse_hex(&obj["act_ptr"]) { self.unit_any.act_ptr = v; }
            if let Some(v) = parse_hex(&obj["path_ptr"]) { self.unit_any.path_ptr = v; }
            if let Some(v) = parse_hex(&obj["next_unit_ptr"]) { self.unit_any.next_unit_ptr = v; }
        }
        if let Some(obj) = json.get("level") {
            if let Some(v) = parse_hex(&obj["level_id"]) { self.level.level_id = v; }
        }

        eprintln!("[map] Offset overrides applied");
    }
}
