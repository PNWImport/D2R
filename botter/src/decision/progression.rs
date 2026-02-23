//! Quest progression engine — the Rust equivalent of kolbot's SoloIndex.js.
//!
//! Encodes the full D2R Act 1-5 quest sequence, level thresholds, and
//! shouldRun/skipIf logic as pure data + functions operating on visual
//! FrameState instead of memory reads.
//!
//! # Design
//!
//! kolbot uses `me.den`, `me.tristram`, `me.charlvl`, `me.gold` etc. — all
//! memory reads. We replace these with:
//!   - `QuestState` (persisted to JSON between games, updated from visual cues)
//!   - `FrameState.char_level` (read from screen)
//!   - `FrameState.area_name_str()` (OCR from gold banner)
//!   - `FrameState.quest_complete_banner` (visual detection)
//!
//! The progression engine decides *what script to run next*. The GameManager
//! decides *how to execute it* (navigate, fight, town, etc.).

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
// D2R AREA NAMES — detected visually from the gold text banner
// that appears at the top-center of screen on area transitions.
// These are the exact strings D2R displays.
// ═══════════════════════════════════════════════════════════════

pub mod areas {
    // Act 1
    pub const ROGUE_ENCAMPMENT: &str = "Rogue Encampment";
    pub const BLOOD_MOOR: &str = "Blood Moor";
    pub const DEN_OF_EVIL: &str = "Den of Evil";
    pub const COLD_PLAINS: &str = "Cold Plains";
    pub const CAVE_LEVEL_1: &str = "Cave Level 1";
    pub const CAVE_LEVEL_2: &str = "Cave Level 2";
    pub const STONY_FIELD: &str = "Stony Field";
    pub const UNDERGROUND_PASSAGE_1: &str = "Underground Passage Level 1";
    pub const UNDERGROUND_PASSAGE_2: &str = "Underground Passage Level 2";
    pub const DARK_WOOD: &str = "Dark Wood";
    pub const BLACK_MARSH: &str = "Black Marsh";
    pub const TRISTRAM: &str = "Tristram";
    pub const BURIAL_GROUNDS: &str = "Burial Grounds";
    pub const MAUSOLEUM: &str = "Mausoleum";
    pub const FORGOTTEN_TOWER: &str = "Forgotten Tower";
    pub const TOWER_CELLAR_5: &str = "Tower Cellar Level 5";
    pub const TAMOE_HIGHLAND: &str = "Tamoe Highland";
    pub const PIT_LEVEL_1: &str = "Pit Level 1";
    pub const PIT_LEVEL_2: &str = "Pit Level 2";
    pub const MONASTERY_GATE: &str = "Monastery Gate";
    pub const OUTER_CLOISTER: &str = "Outer Cloister";
    pub const BARRACKS: &str = "Barracks";
    pub const JAIL_LEVEL_1: &str = "Jail Level 1";
    pub const JAIL_LEVEL_2: &str = "Jail Level 2";
    pub const JAIL_LEVEL_3: &str = "Jail Level 3";
    pub const INNER_CLOISTER: &str = "Inner Cloister";
    pub const CATHEDRAL: &str = "Cathedral";
    pub const CATACOMBS_LEVEL_1: &str = "Catacombs Level 1";
    pub const CATACOMBS_LEVEL_2: &str = "Catacombs Level 2";
    pub const CATACOMBS_LEVEL_3: &str = "Catacombs Level 3";
    pub const CATACOMBS_LEVEL_4: &str = "Catacombs Level 4";
    pub const MOO_MOO_FARM: &str = "Moo Moo Farm";

    // Act 2
    pub const LUT_GHOLEIN: &str = "Lut Gholein";
    pub const SEWERS_LEVEL_1: &str = "Sewers Level 1";
    pub const SEWERS_LEVEL_2: &str = "Sewers Level 2";
    pub const SEWERS_LEVEL_3: &str = "Sewers Level 3";
    pub const ROCKY_WASTE: &str = "Rocky Waste";
    pub const STONY_TOMB_LEVEL_1: &str = "Stony Tomb Level 1";
    pub const STONY_TOMB_LEVEL_2: &str = "Stony Tomb Level 2";
    pub const DRY_HILLS: &str = "Dry Hills";
    pub const HALLS_OF_THE_DEAD_1: &str = "Halls of the Dead Level 1";
    pub const HALLS_OF_THE_DEAD_2: &str = "Halls of the Dead Level 2";
    pub const HALLS_OF_THE_DEAD_3: &str = "Halls of the Dead Level 3";
    pub const FAR_OASIS: &str = "Far Oasis";
    pub const MAGGOT_LAIR_1: &str = "Maggot Lair Level 1";
    pub const MAGGOT_LAIR_2: &str = "Maggot Lair Level 2";
    pub const MAGGOT_LAIR_3: &str = "Maggot Lair Level 3";
    pub const LOST_CITY: &str = "Lost City";
    pub const VALLEY_OF_SNAKES: &str = "Valley of Snakes";
    pub const CLAW_VIPER_TEMPLE_1: &str = "Claw Viper Temple Level 1";
    pub const CLAW_VIPER_TEMPLE_2: &str = "Claw Viper Temple Level 2";
    pub const ANCIENT_TUNNELS: &str = "Ancient Tunnels";
    pub const ARCANE_SANCTUARY: &str = "Arcane Sanctuary";
    pub const CANYON_OF_THE_MAGI: &str = "Canyon of the Magi";
    pub const TALS_TOMBS_PREFIX: &str = "Tal Rasha's"; // all 7 tombs start with this
    pub const DURIELS_LAIR: &str = "Duriel's Lair";

    // Act 3
    pub const KURAST_DOCKS: &str = "Kurast Docks";
    pub const SPIDER_FOREST: &str = "Spider Forest";
    pub const SPIDER_CAVERN: &str = "Spider Cavern";
    pub const GREAT_MARSH: &str = "Great Marsh";
    pub const FLAYER_JUNGLE: &str = "Flayer Jungle";
    pub const FLAYER_DUNGEON_1: &str = "Flayer Dungeon Level 1";
    pub const FLAYER_DUNGEON_2: &str = "Flayer Dungeon Level 2";
    pub const FLAYER_DUNGEON_3: &str = "Flayer Dungeon Level 3";
    pub const LOWER_KURAST: &str = "Lower Kurast";
    pub const KURAST_BAZAAR: &str = "Kurast Bazaar";
    pub const UPPER_KURAST: &str = "Upper Kurast";
    pub const KURAST_SEWERS_1: &str = "Sewers Level 1"; // same name, different act context
    pub const KURAST_CAUSEWAY: &str = "Kurast Causeway";
    pub const TRAVINCAL: &str = "Travincal";
    pub const DURANCE_OF_HATE_1: &str = "Durance of Hate Level 1";
    pub const DURANCE_OF_HATE_2: &str = "Durance of Hate Level 2";
    pub const DURANCE_OF_HATE_3: &str = "Durance of Hate Level 3";
    pub const RUINED_TEMPLE: &str = "Ruined Temple";
    pub const DISUSED_FANE: &str = "Disused Fane";
    pub const FORGOTTEN_RELIQUARY: &str = "Forgotten Reliquary";
    pub const FORGOTTEN_TEMPLE: &str = "Forgotten Temple";
    pub const RUINED_FANE: &str = "Ruined Fane";
    pub const DISUSED_RELIQUARY: &str = "Disused Reliquary";

    // Act 4
    pub const PANDEMONIUM_FORTRESS: &str = "The Pandemonium Fortress";
    pub const OUTER_STEPPES: &str = "Outer Steppes";
    pub const PLAINS_OF_DESPAIR: &str = "Plains of Despair";
    pub const CITY_OF_THE_DAMNED: &str = "City of the Damned";
    pub const RIVER_OF_FLAME: &str = "River of Flame";
    pub const CHAOS_SANCTUARY: &str = "Chaos Sanctuary";

    // Act 5
    pub const HARROGATH: &str = "Harrogath";
    pub const BLOODY_FOOTHILLS: &str = "Bloody Foothills";
    pub const FRIGID_HIGHLANDS: &str = "Frigid Highlands";
    pub const ARREAT_PLATEAU: &str = "Arreat Plateau";
    pub const CRYSTALLINE_PASSAGE: &str = "Crystalline Passage";
    pub const FROZEN_RIVER: &str = "Frozen River";
    pub const GLACIAL_TRAIL: &str = "Glacial Trail";
    pub const FROZEN_TUNDRA: &str = "Frozen Tundra";
    pub const ANCIENTS_WAY: &str = "The Ancients' Way";
    pub const ARREAT_SUMMIT: &str = "Arreat Summit";
    pub const WORLDSTONE_KEEP_1: &str = "Worldstone Keep Level 1";
    pub const WORLDSTONE_KEEP_2: &str = "Worldstone Keep Level 2";
    pub const WORLDSTONE_KEEP_3: &str = "Worldstone Keep Level 3";
    pub const THRONE_OF_DESTRUCTION: &str = "Throne of Destruction";
    pub const WORLDSTONE_CHAMBER: &str = "Worldstone Chamber";
    pub const NIHLATHAKS_TEMPLE: &str = "Nihlathak's Temple";
    pub const HALLS_OF_ANGUISH: &str = "Halls of Anguish";
    pub const HALLS_OF_PAIN: &str = "Halls of Pain";
    pub const HALLS_OF_VAUGHT: &str = "Halls of Vaught";

    /// Determine which act a town area name belongs to.
    pub fn town_act(area: &str) -> Option<u8> {
        match area {
            ROGUE_ENCAMPMENT => Some(1),
            LUT_GHOLEIN => Some(2),
            KURAST_DOCKS => Some(3),
            PANDEMONIUM_FORTRESS => Some(4),
            HARROGATH => Some(5),
            _ => None,
        }
    }

    /// Check if an area name is a town.
    pub fn is_town(area: &str) -> bool {
        town_act(area).is_some()
    }
}

// ═══════════════════════════════════════════════════════════════
// QUEST STATE — persisted to disk between games (like kolbot CharData)
// Updated from visual cues: quest_complete_banner, area transitions,
// NPC dialog detections, etc.
// ═══════════════════════════════════════════════════════════════

/// Persistent quest/progression state for a single character.
/// Saved to JSON after each game, loaded on startup.
/// Field names match kolbot's `me.den`, `me.bloodraven`, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestState {
    // Character identity
    pub character_name: String,
    pub character_class: String,

    // Current progression
    pub level: u8,
    pub difficulty: Difficulty,
    pub current_act: u8,
    pub games_played: u32,

    // ─── Act 1 Quests ────────────────────────────────────────
    pub den: bool,           // Den of Evil completed (skill point reward)
    pub bloodraven: bool,    // Sisters' Burial Grounds completed
    pub tristram: bool,      // The Search for Cain completed (Cain rescued)
    pub countess: bool,      // Forgotten Tower cleared (implied, not a quest)
    pub smith: bool,         // Tools of the Trade completed (Charsi imbue)
    pub andariel: bool,      // Sisters to the Slaughter completed

    // ─── Act 2 Quests ────────────────────────────────────────
    pub radament: bool,      // Radament's Lair completed (skill point)
    pub cube: bool,          // Have Horadric Cube
    pub amulet: bool,        // Have Viper Amulet
    pub shaft: bool,         // Have Staff of Kings
    pub horadricstaff: bool, // Have assembled Horadric Staff
    pub summoner: bool,      // Arcane Sanctuary / Summoner completed
    pub duriel: bool,        // Seven Tombs completed

    // ─── Act 3 Quests ────────────────────────────────────────
    pub lamessen: bool,      // Lam Esen's Tome completed (skill points)
    pub eye: bool,           // Have Khalim's Eye
    pub heart: bool,         // Have Khalim's Heart
    pub brain: bool,         // Have Khalim's Brain
    pub khalimswill: bool,   // Have Khalim's Will (assembled flail)
    pub travincal: bool,     // Khalim's Will / Travincal completed
    pub mephisto: bool,      // The Guardian completed

    // ─── Act 4 Quests ────────────────────────────────────────
    pub izual: bool,         // The Fallen Angel completed (skill points)
    pub hellforge: bool,     // Hell's Forge completed
    pub diablo: bool,        // Terror's End completed

    // ─── Act 5 Quests ────────────────────────────────────────
    pub shenk: bool,         // Siege on Harrogath completed
    pub savebarby: bool,     // Rescue on Mount Arreat completed
    pub anya: bool,          // Prison of Ice completed
    pub ancients: bool,      // Rite of Passage completed
    pub baal: bool,          // Eve of Destruction completed

    // ─── Waypoints obtained ──────────────────────────────────
    // Tracked as a bitfield per difficulty: bit index = waypoint index
    // For now, track the key ones we need for pathing
    pub wp_cold_plains: bool,
    pub wp_stony_field: bool,
    pub wp_dark_wood: bool,
    pub wp_black_marsh: bool,
    pub wp_outer_cloister: bool,
    pub wp_jail_1: bool,
    pub wp_inner_cloister: bool,
    pub wp_catacombs_2: bool,

    // Act 2
    pub wp_sewers_2: bool,
    pub wp_dry_hills: bool,
    pub wp_halls_dead_2: bool,
    pub wp_far_oasis: bool,
    pub wp_lost_city: bool,
    pub wp_arcane_sanctuary: bool,
    pub wp_canyon_of_magi: bool,

    // Act 3
    pub wp_spider_forest: bool,
    pub wp_great_marsh: bool,
    pub wp_flayer_jungle: bool,
    pub wp_lower_kurast: bool,
    pub wp_kurast_bazaar: bool,
    pub wp_upper_kurast: bool,
    pub wp_travincal: bool,
    pub wp_durance_2: bool,

    // Act 4
    pub wp_city_of_damned: bool,
    pub wp_river_of_flame: bool,

    // Act 5
    pub wp_frigid_highlands: bool,
    pub wp_arreat_plateau: bool,
    pub wp_crystalline_passage: bool,
    pub wp_glacial_trail: bool,
    pub wp_halls_of_pain: bool,
    pub wp_frozen_tundra: bool,
    pub wp_ancients_way: bool,
    pub wp_worldstone_keep_2: bool,

    // ─── Misc tracking ───────────────────────────────────────
    pub diff_completed: bool,  // Beat Baal on current difficulty
    pub gold_low_streak: u8,   // consecutive games with low gold (kolbot Check.brokeAf())
}

impl Default for QuestState {
    fn default() -> Self {
        Self {
            character_name: String::new(),
            character_class: "Sorceress".into(),
            level: 1,
            difficulty: Difficulty::Normal,
            current_act: 1,
            games_played: 0,
            // All quests start incomplete
            den: false, bloodraven: false, tristram: false, countess: false,
            smith: false, andariel: false,
            radament: false, cube: false, amulet: false, shaft: false,
            horadricstaff: false, summoner: false, duriel: false,
            lamessen: false, eye: false, heart: false, brain: false,
            khalimswill: false, travincal: false, mephisto: false,
            izual: false, hellforge: false, diablo: false,
            shenk: false, savebarby: false, anya: false, ancients: false, baal: false,
            // No waypoints
            wp_cold_plains: false, wp_stony_field: false, wp_dark_wood: false,
            wp_black_marsh: false, wp_outer_cloister: false, wp_jail_1: false,
            wp_inner_cloister: false, wp_catacombs_2: false,
            wp_sewers_2: false, wp_dry_hills: false, wp_halls_dead_2: false,
            wp_far_oasis: false, wp_lost_city: false, wp_arcane_sanctuary: false,
            wp_canyon_of_magi: false,
            wp_spider_forest: false, wp_great_marsh: false, wp_flayer_jungle: false,
            wp_lower_kurast: false, wp_kurast_bazaar: false, wp_upper_kurast: false,
            wp_travincal: false, wp_durance_2: false,
            wp_city_of_damned: false, wp_river_of_flame: false,
            wp_frigid_highlands: false, wp_arreat_plateau: false,
            wp_crystalline_passage: false, wp_glacial_trail: false,
            wp_halls_of_pain: false, wp_frozen_tundra: false,
            wp_ancients_way: false, wp_worldstone_keep_2: false,
            diff_completed: false,
            gold_low_streak: 0,
        }
    }
}

impl QuestState {
    /// Load from a JSON file. Returns default if file doesn't exist.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save to a JSON file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, content)
    }

    /// Whether we have access to a given act (based on quest completions).
    pub fn access_to_act(&self, act: u8) -> bool {
        match act {
            1 => true,
            2 => self.andariel,
            3 => self.duriel,
            4 => self.mephisto, // need to beat Travincal + Mephisto
            5 => self.diablo,
            _ => false,
        }
    }

    /// kolbot equivalent of `Check.brokeAf()` — are we critically low on gold?
    pub fn broke_af(&self) -> bool {
        self.gold_low_streak >= 3
    }

    /// Update level from visual detection (only increases, never decreases).
    pub fn update_level(&mut self, detected_level: u8) {
        if detected_level > self.level && detected_level <= 99 {
            self.level = detected_level;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Normal,
    Nightmare,
    Hell,
}

impl Difficulty {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Nightmare => 1,
            Self::Hell => 2,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Nightmare,
            2 => Self::Hell,
            _ => Self::Normal,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// SCRIPT DEFINITIONS — what the bot can do
// ═══════════════════════════════════════════════════════════════

/// A script the progression engine can select to run.
/// Each script represents a game objective (quest, farming run, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    // Act 1
    Den,
    Bishibosh,
    Cave,
    BloodRaven,
    Tristram,
    Treehead,
    Countess,
    Smith,
    Pits,
    BoneAsh,
    Andariel,
    Cows,

    // Act 2
    Cube,
    Radament,
    CreepingFeature,
    BeetleBurst,
    Amulet,
    Staff,
    Summoner,
    FireEye,
    MaggotLair,
    Tombs,
    AncientTunnels,
    Duriel,

    // Act 3
    LamEssen,
    TempleRuns,
    LowerKurast,
    Eye,
    Heart,
    Brain,
    Travincal,
    Mephisto,

    // Act 4
    Izual,
    HellForge,
    River,
    Hephasto,
    Diablo,

    // Act 5
    Shenk,
    SaveBarby,
    Anya,
    Pindle,
    Nith,
    Ancients,
    Baal,
}

impl Script {
    /// The act this script belongs to.
    pub fn act(self) -> u8 {
        match self {
            Self::Den | Self::Bishibosh | Self::Cave | Self::BloodRaven
            | Self::Tristram | Self::Treehead | Self::Countess | Self::Smith
            | Self::Pits | Self::BoneAsh | Self::Andariel | Self::Cows => 1,

            Self::Cube | Self::Radament | Self::CreepingFeature | Self::BeetleBurst
            | Self::Amulet | Self::Staff | Self::Summoner | Self::FireEye
            | Self::MaggotLair | Self::Tombs | Self::AncientTunnels | Self::Duriel => 2,

            Self::LamEssen | Self::TempleRuns | Self::LowerKurast
            | Self::Eye | Self::Heart | Self::Brain
            | Self::Travincal | Self::Mephisto => 3,

            Self::Izual | Self::HellForge | Self::River
            | Self::Hephasto | Self::Diablo => 4,

            Self::Shenk | Self::SaveBarby | Self::Anya | Self::Pindle
            | Self::Nith | Self::Ancients | Self::Baal => 5,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Den => "den",
            Self::Bishibosh => "bishibosh",
            Self::Cave => "cave",
            Self::BloodRaven => "bloodraven",
            Self::Tristram => "tristram",
            Self::Treehead => "treehead",
            Self::Countess => "countess",
            Self::Smith => "smith",
            Self::Pits => "pits",
            Self::BoneAsh => "boneash",
            Self::Andariel => "andariel",
            Self::Cows => "cows",
            Self::Cube => "cube",
            Self::Radament => "radament",
            Self::CreepingFeature => "creepingfeature",
            Self::BeetleBurst => "beetleburst",
            Self::Amulet => "amulet",
            Self::Staff => "staff",
            Self::Summoner => "summoner",
            Self::FireEye => "fireeye",
            Self::MaggotLair => "maggotlair",
            Self::Tombs => "tombs",
            Self::AncientTunnels => "ancienttunnels",
            Self::Duriel => "duriel",
            Self::LamEssen => "lamessen",
            Self::TempleRuns => "templeruns",
            Self::LowerKurast => "lowerkurast",
            Self::Eye => "eye",
            Self::Heart => "heart",
            Self::Brain => "brain",
            Self::Travincal => "travincal",
            Self::Mephisto => "mephisto",
            Self::Izual => "izual",
            Self::HellForge => "hellforge",
            Self::River => "river",
            Self::Hephasto => "hephasto",
            Self::Diablo => "diablo",
            Self::Shenk => "shenk",
            Self::SaveBarby => "savebarby",
            Self::Anya => "anya",
            Self::Pindle => "pindle",
            Self::Nith => "nith",
            Self::Ancients => "ancients",
            Self::Baal => "baal",
        }
    }
}

/// The ordered script sequence — mirrors kolbot SoloIndex.scripts exactly.
/// This is the master order that the progression engine iterates through.
pub const SCRIPT_SEQUENCE: &[Script] = &[
    // Act 1
    Script::Den, Script::Bishibosh, Script::BloodRaven, Script::Tristram,
    Script::Treehead, Script::Countess, Script::Smith, Script::Pits,
    Script::BoneAsh, Script::Andariel, Script::Cows,
    // Act 2
    Script::Cube, Script::Radament, Script::CreepingFeature, Script::BeetleBurst,
    Script::Amulet, Script::Summoner, Script::FireEye, Script::MaggotLair,
    Script::Tombs, Script::AncientTunnels, Script::Staff, Script::Duriel,
    // Act 3
    Script::LamEssen, Script::TempleRuns, Script::LowerKurast,
    Script::Eye, Script::Heart, Script::Brain, Script::Travincal, Script::Mephisto,
    // Act 4
    Script::Izual, Script::HellForge, Script::River, Script::Hephasto, Script::Diablo,
    // Act 5
    Script::Shenk, Script::SaveBarby, Script::Anya, Script::Pindle,
    Script::Nith, Script::Ancients, Script::Baal,
];

// ═══════════════════════════════════════════════════════════════
// SHOULD-RUN LOGIC — ported from kolbot SoloIndex.index
// ═══════════════════════════════════════════════════════════════

/// Check whether a script should run given current quest state and character info.
/// This is the Rust equivalent of kolbot's `shouldRun()` + `skipIf()` + `preReq()`
/// for each script in SoloIndex.index.
///
/// `class` is the character class name (e.g. "Sorceress", "Barbarian").
/// `can_teleport` indicates if the character has teleport available.
pub fn should_run(
    script: Script,
    qs: &QuestState,
    class: &str,
    can_teleport: bool,
) -> bool {
    let lvl = qs.level;
    let diff = qs.difficulty;
    let broke = qs.broke_af();

    match script {
        // ─── Act 1 ───────────────────────────────────────────

        Script::Den => {
            // kolbot: skipIf: me.den || (charlvl > 8 && charlvl < (sorceress ? 18 : 12))
            if qs.den { return false; }
            let skip_range = if class == "Sorceress" { 18 } else { 12 };
            if lvl > 8 && lvl < skip_range { return false; }
            true
        }

        Script::Bishibosh => {
            // kolbot: preReq: charlvl > 10, skipIf: sorceress
            // For Sorceress, Bishibosh is called as a sub-script from Den
            // (handled internally by den execution). Standalone only for non-sorc lvl 10+.
            if class == "Sorceress" { return false; }
            if lvl <= 10 { return false; }
            broke
        }

        Script::Cave => {
            // Cave is a sub-script called from Den for early leveling.
            // Not run standalone from the main sequence.
            false
        }

        Script::BloodRaven => {
            // kolbot Normal: !bloodraven || (!summoner && brokeAf) || (!tristram && barbarian)
            match diff {
                Difficulty::Normal => {
                    !qs.bloodraven
                    || (!qs.summoner && broke)
                    || (!qs.tristram && class == "Barbarian")
                }
                Difficulty::Nightmare => !qs.bloodraven,
                Difficulty::Hell => {
                    // Skip for Lightning/Trapsin builds (too many light immunes)
                    // For now, always run in hell unless class-specific skip
                    true
                }
            }
        }

        Script::Tristram => {
            // kolbot: complex shouldRun based on difficulty + level
            if lvl < 6 { return false; } // game timer check not applicable here
            match diff {
                Difficulty::Normal => {
                    !qs.tristram || lvl < 12 || broke
                }
                Difficulty::Nightmare => {
                    (!qs.tristram && lvl < 43) || broke
                }
                Difficulty::Hell => {
                    !qs.tristram || (lvl <= 72)
                }
            }
        }

        Script::Treehead => {
            // kolbot: preReq: hell && !accessToAct(3), skipIf: !paladin
            if diff != Difficulty::Hell { return false; }
            if qs.access_to_act(3) { return false; }
            class == "Paladin"
        }

        Script::Countess => {
            // kolbot: skip in hell for classic/sorc-not-final.
            // shouldRun: normal && (needRunes || brokeAf), or NM/Hell with tele
            if diff == Difficulty::Hell && class == "Sorceress" { return false; }
            match diff {
                Difficulty::Normal => broke || lvl < 20, // rune check simplified to level
                Difficulty::Nightmare | Difficulty::Hell => can_teleport || lvl < 60,
            }
        }

        Script::Smith => {
            // kolbot: preReq: charlvl > 6, skipIf: quest done
            if lvl <= 6 { return false; }
            !qs.smith
        }

        Script::Pits => {
            // kolbot: preReq: hell, skipIf: class-specific level checks
            if diff != Difficulty::Hell { return false; }
            lvl >= 85
        }

        Script::BoneAsh => {
            // kolbot: skipIf: charlvl < 10, shouldRun: charlvl < 12 || brokeAf
            if lvl < 10 { return false; }
            lvl < 12 || broke
        }

        Script::Andariel => {
            // kolbot: skipIf: charlvl < 11
            if lvl < 11 { return false; }
            match diff {
                Difficulty::Normal => !qs.andariel || broke,
                Difficulty::Nightmare | Difficulty::Hell => {
                    !qs.andariel || can_teleport || lvl < 60
                }
            }
        }

        Script::Cows => {
            // kolbot: preReq: !cows && diffCompleted
            if !qs.diff_completed { return false; }
            if diff == Difficulty::Normal && !broke { return false; }
            true
        }

        // ─── Act 2 ───────────────────────────────────────────

        Script::Cube => {
            if !qs.access_to_act(2) { return false; }
            if qs.cube { return false; }
            if class == "Sorceress" && lvl < 18 { return false; } // wait for tele
            true
        }

        Script::Radament => {
            if !qs.access_to_act(2) { return false; }
            !qs.radament || (diff == Difficulty::Normal && broke)
        }

        Script::CreepingFeature => {
            if !qs.access_to_act(2) { return false; }
            lvl >= 12 && lvl <= 20
        }

        Script::BeetleBurst => {
            if !qs.access_to_act(2) { return false; }
            lvl >= 12 && lvl <= 20
        }

        Script::Amulet => {
            if !qs.access_to_act(2) { return false; }
            !(qs.horadricstaff || qs.amulet)
        }

        Script::Staff => {
            if !qs.access_to_act(2) { return false; }
            !(qs.horadricstaff || qs.shaft)
        }

        Script::Summoner => {
            if !qs.access_to_act(2) { return false; }
            !qs.summoner
        }

        Script::FireEye => {
            if !qs.access_to_act(2) { return false; }
            if qs.summoner { return false; }
            lvl >= 16 && lvl <= 23
        }

        Script::MaggotLair => {
            if !qs.access_to_act(2) { return false; }
            if !can_teleport { return false; }
            diff == Difficulty::Normal && lvl <= 21
        }

        Script::Tombs => {
            if !qs.access_to_act(2) || !qs.summoner { return false; }
            diff == Difficulty::Normal && lvl <= 22
        }

        Script::AncientTunnels => {
            if diff != Difficulty::Hell { return false; }
            if !qs.access_to_act(2) { return false; }
            true
        }

        Script::Duriel => {
            if !qs.access_to_act(2) { return false; }
            if qs.duriel { return false; }
            // Need assembled staff or both pieces
            qs.horadricstaff || (qs.amulet && qs.shaft)
        }

        // ─── Act 3 ───────────────────────────────────────────

        Script::LamEssen => {
            if !qs.access_to_act(3) { return false; }
            !qs.lamessen
        }

        Script::TempleRuns => {
            if !qs.access_to_act(3) { return false; }
            match diff {
                Difficulty::Normal => lvl > 18 && lvl < 25,
                Difficulty::Nightmare => lvl < 50,
                Difficulty::Hell => lvl > 80,
            }
        }

        Script::LowerKurast => {
            if !qs.access_to_act(3) { return false; }
            match (class, diff) {
                ("Sorceress", Difficulty::Hell) => lvl < 90,
                ("Barbarian", Difficulty::Nightmare) => lvl >= 50,
                _ => false,
            }
        }

        Script::Eye => {
            if !qs.access_to_act(3) { return false; }
            !(qs.eye || qs.khalimswill || qs.travincal)
        }

        Script::Heart => {
            if !qs.access_to_act(3) { return false; }
            !(qs.heart || qs.khalimswill || qs.travincal)
        }

        Script::Brain => {
            if !qs.access_to_act(3) { return false; }
            !(qs.brain || qs.khalimswill || qs.travincal)
        }

        Script::Travincal => {
            if !qs.access_to_act(3) { return false; }
            !qs.travincal || (lvl < 25) || broke
        }

        Script::Mephisto => {
            if !qs.access_to_act(3) || !qs.travincal { return false; }
            match diff {
                Difficulty::Normal => !qs.mephisto || broke,
                Difficulty::Nightmare => can_teleport || lvl <= 65,
                Difficulty::Hell => can_teleport,
            }
        }

        // ─── Act 4 ───────────────────────────────────────────

        Script::Izual => {
            if !qs.access_to_act(4) { return false; }
            !qs.izual || (diff == Difficulty::Normal && !qs.diablo)
        }

        Script::HellForge => {
            if !qs.access_to_act(4) { return false; }
            !qs.hellforge
        }

        Script::River => {
            if !qs.access_to_act(4) { return false; }
            if qs.diablo || diff == Difficulty::Normal { return false; }
            let min_lvl = match diff {
                Difficulty::Normal => 24,
                Difficulty::Nightmare => 40,
                Difficulty::Hell => 80,
            };
            lvl >= min_lvl && (class == "Barbarian" || class == "Sorceress")
        }

        Script::Hephasto => {
            if !qs.access_to_act(4) { return false; }
            if class != "Barbarian" || diff == Difficulty::Normal || qs.diablo { return false; }
            lvl <= 70
        }

        Script::Diablo => {
            if !qs.access_to_act(4) { return false; }
            let min_lvl = match diff {
                Difficulty::Normal => 24,
                Difficulty::Nightmare => 40,
                Difficulty::Hell => 80,
            };
            if lvl < min_lvl { return false; }
            if !qs.diablo { return true; }
            match diff {
                Difficulty::Normal => lvl < 30 || !qs.diff_completed,
                Difficulty::Nightmare => can_teleport || lvl <= 65,
                Difficulty::Hell => true,
            }
        }

        // ─── Act 5 ───────────────────────────────────────────

        Script::Shenk => {
            if !qs.access_to_act(5) { return false; }
            !qs.shenk || lvl <= 70
        }

        Script::SaveBarby => {
            if !qs.access_to_act(5) { return false; }
            !qs.savebarby
        }

        Script::Anya => {
            if !qs.access_to_act(5) { return false; }
            !qs.anya
        }

        Script::Pindle => {
            if !qs.access_to_act(5) || !qs.anya { return false; }
            true
        }

        Script::Nith => {
            if !qs.access_to_act(5) || !qs.anya { return false; }
            if !can_teleport { return false; }
            if diff == Difficulty::Normal && lvl < 30 { return false; }
            diff != Difficulty::Hell // for now only norm/nm
        }

        Script::Ancients => {
            if !qs.access_to_act(5) { return false; }
            !qs.ancients
        }

        Script::Baal => {
            if !qs.access_to_act(5) { return false; }
            qs.ancients // must beat ancients first
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// PROGRESSION ENGINE — selects the next script to run
// ═══════════════════════════════════════════════════════════════

/// The progression engine. Iterates through SCRIPT_SEQUENCE, evaluates
/// should_run() for each, and returns the next script to execute.
pub struct ProgressionEngine {
    pub quest_state: QuestState,
    /// Scripts completed this game (reset each game).
    done_this_game: Vec<Script>,
    /// Scripts to retry next game (like kolbot retryList).
    retry_list: Vec<Script>,
    /// Current index into SCRIPT_SEQUENCE.
    sequence_index: usize,
    /// Character class (from config).
    class: String,
    /// Can the character teleport? (updated from config/level).
    can_teleport: bool,
    /// Path to quest state JSON file.
    state_path: std::path::PathBuf,
    /// When the current game started.
    game_start: Instant,
}

impl ProgressionEngine {
    pub fn new(class: String, state_path: std::path::PathBuf) -> Self {
        let quest_state = QuestState::load(&state_path);
        Self {
            quest_state,
            done_this_game: Vec::new(),
            retry_list: Vec::new(),
            sequence_index: 0,
            class,
            can_teleport: false,
            state_path,
            game_start: Instant::now(),
        }
    }

    /// Set whether the character can teleport (from config or level check).
    pub fn set_can_teleport(&mut self, can_tele: bool) {
        self.can_teleport = can_tele;
    }

    /// Called when a new game starts. Resets per-game state.
    pub fn on_game_start(&mut self) {
        self.done_this_game.clear();
        self.sequence_index = 0;
        self.game_start = Instant::now();
        self.quest_state.games_played += 1;
    }

    /// Called when a game ends. Saves quest state to disk.
    pub fn on_game_end(&mut self) {
        if let Err(e) = self.quest_state.save(&self.state_path) {
            tracing::error!("Failed to save quest state: {}", e);
        }
    }

    /// Select the next script to run.
    /// Returns None if all scripts have been evaluated (game should end).
    pub fn next_script(&mut self) -> Option<Script> {
        // First, check retry list (scripts that need to be re-run)
        if let Some(script) = self.retry_list.pop() {
            if should_run(script, &self.quest_state, &self.class, self.can_teleport) {
                return Some(script);
            }
        }

        // Then iterate through the main sequence
        while self.sequence_index < SCRIPT_SEQUENCE.len() {
            let script = SCRIPT_SEQUENCE[self.sequence_index];
            self.sequence_index += 1;

            // Skip if already done this game
            if self.done_this_game.contains(&script) {
                continue;
            }

            if should_run(script, &self.quest_state, &self.class, self.can_teleport) {
                return Some(script);
            }
        }

        None // All scripts evaluated — end game
    }

    /// Mark a script as completed this game.
    pub fn mark_done(&mut self, script: Script) {
        if !self.done_this_game.contains(&script) {
            self.done_this_game.push(script);
        }
        tracing::info!("Script {} completed", script.name());
    }

    /// Push a script to the retry list (will be tried first next game).
    /// kolbot equivalent of `SoloIndex.retryList.push()`.
    pub fn retry_next_game(&mut self, script: Script) {
        if !self.retry_list.contains(&script) {
            self.retry_list.push(script);
        }
    }

    /// Update quest state from a visual cue.
    /// Called by the vision pipeline when it detects quest completion banners,
    /// area transitions, or NPC dialog results.
    pub fn on_quest_complete(&mut self, script: Script) {
        match script {
            Script::Den => self.quest_state.den = true,
            Script::BloodRaven => self.quest_state.bloodraven = true,
            Script::Tristram => self.quest_state.tristram = true,
            Script::Smith => self.quest_state.smith = true,
            Script::Andariel => {
                self.quest_state.andariel = true;
                self.quest_state.current_act = 2;
            }
            Script::Radament => self.quest_state.radament = true,
            Script::Cube => self.quest_state.cube = true,
            Script::Summoner => self.quest_state.summoner = true,
            Script::Duriel => {
                self.quest_state.duriel = true;
                self.quest_state.current_act = 3;
            }
            Script::LamEssen => self.quest_state.lamessen = true,
            Script::Eye => self.quest_state.eye = true,
            Script::Heart => self.quest_state.heart = true,
            Script::Brain => self.quest_state.brain = true,
            Script::Travincal => {
                self.quest_state.travincal = true;
                self.quest_state.khalimswill = true;
            }
            Script::Mephisto => {
                self.quest_state.mephisto = true;
                self.quest_state.current_act = 4;
            }
            Script::Izual => self.quest_state.izual = true,
            Script::HellForge => self.quest_state.hellforge = true,
            Script::Diablo => {
                self.quest_state.diablo = true;
                self.quest_state.current_act = 5;
            }
            Script::Shenk => self.quest_state.shenk = true,
            Script::SaveBarby => self.quest_state.savebarby = true,
            Script::Anya => self.quest_state.anya = true,
            Script::Ancients => self.quest_state.ancients = true,
            Script::Baal => {
                self.quest_state.baal = true;
                self.quest_state.diff_completed = true;
            }
            _ => {} // Scripts without quest flags (farming runs)
        }

        // Auto-save after quest completion
        if let Err(e) = self.quest_state.save(&self.state_path) {
            tracing::error!("Failed to save quest state: {}", e);
        }
    }

    /// Update quest state when a waypoint is visually detected as obtained.
    pub fn on_waypoint_obtained(&mut self, area: &str) {
        match area {
            areas::COLD_PLAINS => self.quest_state.wp_cold_plains = true,
            areas::STONY_FIELD => self.quest_state.wp_stony_field = true,
            areas::DARK_WOOD => self.quest_state.wp_dark_wood = true,
            areas::BLACK_MARSH => self.quest_state.wp_black_marsh = true,
            areas::OUTER_CLOISTER => self.quest_state.wp_outer_cloister = true,
            areas::JAIL_LEVEL_1 => self.quest_state.wp_jail_1 = true,
            areas::INNER_CLOISTER => self.quest_state.wp_inner_cloister = true,
            areas::CATACOMBS_LEVEL_2 => self.quest_state.wp_catacombs_2 = true,
            areas::TRAVINCAL => self.quest_state.wp_travincal = true,
            areas::RIVER_OF_FLAME => self.quest_state.wp_river_of_flame = true,
            _ => {}
        }
    }

    /// Get the current quest state (read-only).
    pub fn state(&self) -> &QuestState {
        &self.quest_state
    }
}

// ═══════════════════════════════════════════════════════════════
// SCRIPT EXECUTION PLANS — what each script needs to DO visually
// ═══════════════════════════════════════════════════════════════

/// High-level steps for executing a script via screen interaction.
/// The GameManager interprets these into actual clicks/keypresses.
#[derive(Debug, Clone)]
pub enum ScriptStep {
    /// Do town chores (heal, repair, buy pots, etc.)
    TownChores,
    /// Use waypoint to travel to a specific area.
    /// The string is the area name to select in the WP menu.
    UseWaypoint { destination: &'static str },
    /// Walk toward an area exit (navigate toward edge of screen in a direction).
    WalkToExit { target_area: &'static str },
    /// Clear current area of monsters (full clear or targeted).
    ClearArea,
    /// Kill a specific boss/super unique by name.
    KillTarget { name: &'static str },
    /// Pick up items after clearing.
    LootArea,
    /// Open Town Portal and go to town.
    TownPortal,
    /// Talk to an NPC (by walking to their known position).
    TalkToNpc { npc: &'static str, act: u8 },
    /// Interact with a game object (waypoint, chest, cairn stone, etc.)
    InteractObject { name: &'static str },
    /// Wait for a visual cue (quest complete banner, area transition, etc.)
    WaitForCue { cue: VisualCue, timeout_secs: u8 },
    /// Check if character level meets a threshold (for gating den entry etc.)
    RequireLevel { min_level: u8 },
    /// If level check fails, retry this script next game.
    RetryNextGame,
}

/// Visual cues the bot can wait for.
#[derive(Debug, Clone, Copy)]
pub enum VisualCue {
    QuestCompleteBanner,
    AreaTransition,
    NpcDialogOpen,
    WaypointMenuOpen,
    LoadingScreenEnd,
}

/// Get the execution plan for a script.
/// Returns the ordered list of steps to perform.
pub fn script_plan(script: Script, qs: &QuestState) -> Vec<ScriptStep> {
    match script {
        Script::Den => den_plan(qs),
        Script::BloodRaven => bloodraven_plan(qs),
        Script::Tristram => tristram_plan(qs),
        Script::Countess => countess_plan(qs),
        Script::Smith => smith_plan(qs),
        Script::Andariel => andariel_plan(qs),
        _ => {
            // Default plan for unimplemented scripts: town chores + clear
            vec![
                ScriptStep::TownChores,
                ScriptStep::ClearArea,
                ScriptStep::LootArea,
                ScriptStep::TownPortal,
            ]
        }
    }
}

// ─── Act 1 Script Plans ──────────────────────────────────────

fn den_plan(qs: &QuestState) -> Vec<ScriptStep> {
    let mut steps = Vec::new();

    // Level gate: kolbot won't enter den until level 8
    steps.push(ScriptStep::RequireLevel { min_level: 8 });

    // If under level 8, we need to farm first (cave, bishibosh)
    // The retry mechanism handles this — if RequireLevel fails,
    // the script runner pushes a RetryNextGame.

    steps.push(ScriptStep::TownChores);

    // Navigate to Blood Moor
    if !qs.wp_cold_plains {
        // First time: walk from town through Blood Moor to get Cold Plains WP
        steps.push(ScriptStep::WalkToExit { target_area: areas::BLOOD_MOOR });
        steps.push(ScriptStep::ClearArea); // kill along the way for XP
        steps.push(ScriptStep::WalkToExit { target_area: areas::COLD_PLAINS });
        steps.push(ScriptStep::InteractObject { name: "waypoint" });
    } else {
        // Have Cold Plains WP: use it then walk back to Blood Moor
        steps.push(ScriptStep::WalkToExit { target_area: areas::BLOOD_MOOR });
    }

    // Enter Den of Evil
    steps.push(ScriptStep::WalkToExit { target_area: areas::DEN_OF_EVIL });
    steps.push(ScriptStep::WaitForCue {
        cue: VisualCue::AreaTransition,
        timeout_secs: 30,
    });

    // Clear the den
    steps.push(ScriptStep::ClearArea);
    steps.push(ScriptStep::LootArea);

    // Wait for quest complete visual (the lights change + banner)
    steps.push(ScriptStep::WaitForCue {
        cue: VisualCue::QuestCompleteBanner,
        timeout_secs: 10,
    });

    // Return to town and talk to Akara for skill reward
    steps.push(ScriptStep::TownPortal);
    steps.push(ScriptStep::TalkToNpc { npc: "Akara", act: 1 });

    steps
}

fn bloodraven_plan(_qs: &QuestState) -> Vec<ScriptStep> {
    vec![
        ScriptStep::TownChores,
        ScriptStep::WalkToExit { target_area: areas::COLD_PLAINS },
        ScriptStep::WalkToExit { target_area: areas::BURIAL_GROUNDS },
        ScriptStep::WaitForCue {
            cue: VisualCue::AreaTransition,
            timeout_secs: 30,
        },
        ScriptStep::KillTarget { name: "Blood Raven" },
        ScriptStep::LootArea,
        ScriptStep::WaitForCue {
            cue: VisualCue::QuestCompleteBanner,
            timeout_secs: 10,
        },
        ScriptStep::TownPortal,
        // Talk to Kashya for rogue merc reward
        ScriptStep::TalkToNpc { npc: "Kashya", act: 1 },
    ]
}

fn tristram_plan(qs: &QuestState) -> Vec<ScriptStep> {
    let mut steps = Vec::new();
    steps.push(ScriptStep::TownChores);

    if !qs.tristram {
        // Full Tristram quest: get scroll → decode → activate stones → rescue Cain

        // 1. Get Scroll of Inifuss from Dark Wood
        if qs.wp_dark_wood {
            steps.push(ScriptStep::UseWaypoint { destination: areas::DARK_WOOD });
        } else if qs.wp_black_marsh {
            steps.push(ScriptStep::UseWaypoint { destination: areas::BLACK_MARSH });
            steps.push(ScriptStep::WalkToExit { target_area: areas::DARK_WOOD });
        } else {
            // Walk from Stony Field through Underground Passage
            steps.push(ScriptStep::UseWaypoint { destination: areas::STONY_FIELD });
            steps.push(ScriptStep::WalkToExit { target_area: areas::UNDERGROUND_PASSAGE_1 });
            steps.push(ScriptStep::WalkToExit { target_area: areas::DARK_WOOD });
        }

        // Find and click the Tree of Inifuss
        steps.push(ScriptStep::InteractObject { name: "Tree of Inifuss" });
        steps.push(ScriptStep::LootArea); // pick up scroll

        // Get Black Marsh WP if we don't have it
        if !qs.wp_black_marsh {
            steps.push(ScriptStep::WalkToExit { target_area: areas::BLACK_MARSH });
            steps.push(ScriptStep::InteractObject { name: "waypoint" });
        }

        // 2. Return to town, talk to Akara to decode scroll
        steps.push(ScriptStep::TownPortal);
        steps.push(ScriptStep::TalkToNpc { npc: "Akara", act: 1 });

        // 3. Go to Stony Field, find Cairn Stones
        steps.push(ScriptStep::UseWaypoint { destination: areas::STONY_FIELD });
        // Kill Rakanishu near the stones
        steps.push(ScriptStep::KillTarget { name: "Rakanishu" });
        // Activate the 5 Cairn Stones
        for _ in 1..=5 {
            steps.push(ScriptStep::InteractObject { name: "Cairn Stone" });
        }

        // 4. Enter Tristram portal
        steps.push(ScriptStep::WaitForCue {
            cue: VisualCue::AreaTransition,
            timeout_secs: 30,
        });
    } else {
        // Already rescued Cain — just go to Tristram for farming
        steps.push(ScriptStep::UseWaypoint { destination: areas::STONY_FIELD });
        steps.push(ScriptStep::KillTarget { name: "Rakanishu" });
        // Cairn Stones should auto-activate if quest is done
        for _ in 1..=5 {
            steps.push(ScriptStep::InteractObject { name: "Cairn Stone" });
        }
        steps.push(ScriptStep::WaitForCue {
            cue: VisualCue::AreaTransition,
            timeout_secs: 30,
        });
    }

    // In Tristram: clear everything + rescue Cain if needed
    if !qs.tristram {
        steps.push(ScriptStep::ClearArea);
        steps.push(ScriptStep::InteractObject { name: "Cain's Gibbet" });
        steps.push(ScriptStep::WaitForCue {
            cue: VisualCue::QuestCompleteBanner,
            timeout_secs: 10,
        });
    }

    steps.push(ScriptStep::ClearArea);
    steps.push(ScriptStep::LootArea);
    steps.push(ScriptStep::TownPortal);

    // Talk to Akara in town (Cain should now be in camp)
    if !qs.tristram {
        steps.push(ScriptStep::TalkToNpc { npc: "Akara", act: 1 });
    }

    steps
}

fn countess_plan(_qs: &QuestState) -> Vec<ScriptStep> {
    vec![
        ScriptStep::TownChores,
        ScriptStep::UseWaypoint { destination: areas::BLACK_MARSH },
        ScriptStep::WalkToExit { target_area: areas::FORGOTTEN_TOWER },
        ScriptStep::WaitForCue {
            cue: VisualCue::AreaTransition,
            timeout_secs: 30,
        },
        // Navigate down through Tower Cellar levels 1-5
        ScriptStep::WalkToExit { target_area: areas::TOWER_CELLAR_5 },
        ScriptStep::KillTarget { name: "The Countess" },
        ScriptStep::LootArea,
        ScriptStep::TownPortal,
    ]
}

fn smith_plan(_qs: &QuestState) -> Vec<ScriptStep> {
    vec![
        ScriptStep::TownChores,
        ScriptStep::UseWaypoint { destination: areas::BARRACKS },
        ScriptStep::KillTarget { name: "The Smith" },
        ScriptStep::LootArea,
        // Pick up Horadric Malus
        ScriptStep::InteractObject { name: "Horadric Malus" },
        ScriptStep::TownPortal,
        // Talk to Charsi for imbue reward
        ScriptStep::TalkToNpc { npc: "Charsi", act: 1 },
    ]
}

fn andariel_plan(_qs: &QuestState) -> Vec<ScriptStep> {
    vec![
        ScriptStep::TownChores,
        ScriptStep::UseWaypoint { destination: areas::CATACOMBS_LEVEL_2 },
        ScriptStep::WalkToExit { target_area: areas::CATACOMBS_LEVEL_3 },
        ScriptStep::WalkToExit { target_area: areas::CATACOMBS_LEVEL_4 },
        ScriptStep::WaitForCue {
            cue: VisualCue::AreaTransition,
            timeout_secs: 30,
        },
        ScriptStep::ClearArea,
        ScriptStep::KillTarget { name: "Andariel" },
        ScriptStep::LootArea,
        ScriptStep::WaitForCue {
            cue: VisualCue::QuestCompleteBanner,
            timeout_secs: 15,
        },
        ScriptStep::TownPortal,
        // Talk to Warriv to travel to Act 2
        ScriptStep::TalkToNpc { npc: "Warriv", act: 1 },
    ]
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_character_runs_den() {
        let qs = QuestState::default();
        assert!(should_run(Script::Den, &qs, "Sorceress", false));
    }

    #[test]
    fn test_den_skipped_after_completion() {
        let mut qs = QuestState::default();
        qs.den = true;
        assert!(!should_run(Script::Den, &qs, "Sorceress", false));
    }

    #[test]
    fn test_den_skipped_mid_levels_sorc() {
        let mut qs = QuestState::default();
        qs.level = 12; // between 8 and 18 for sorc = skip
        assert!(!should_run(Script::Den, &qs, "Sorceress", false));
    }

    #[test]
    fn test_den_runs_at_level_18_sorc() {
        let mut qs = QuestState::default();
        qs.level = 18; // >= 18 for sorc = run (has tele)
        assert!(should_run(Script::Den, &qs, "Sorceress", false));
    }

    #[test]
    fn test_andariel_requires_level_11() {
        let mut qs = QuestState::default();
        qs.level = 10;
        assert!(!should_run(Script::Andariel, &qs, "Sorceress", false));
        qs.level = 11;
        assert!(should_run(Script::Andariel, &qs, "Sorceress", false));
    }

    #[test]
    fn test_act2_requires_andariel() {
        let qs = QuestState::default(); // andariel = false
        assert!(!should_run(Script::Radament, &qs, "Sorceress", false));
        assert!(!should_run(Script::Cube, &qs, "Sorceress", false));
    }

    #[test]
    fn test_act2_accessible_after_andariel() {
        let mut qs = QuestState::default();
        qs.andariel = true;
        qs.level = 20;
        assert!(should_run(Script::Radament, &qs, "Sorceress", false));
    }

    #[test]
    fn test_tristram_requires_level_6() {
        let mut qs = QuestState::default();
        qs.level = 5;
        assert!(!should_run(Script::Tristram, &qs, "Sorceress", false));
        qs.level = 6;
        assert!(should_run(Script::Tristram, &qs, "Sorceress", false));
    }

    #[test]
    fn test_baal_requires_ancients() {
        let mut qs = QuestState::default();
        qs.diablo = true; // can access act 5
        qs.level = 80;
        assert!(!should_run(Script::Baal, &qs, "Sorceress", true));
        qs.ancients = true;
        assert!(should_run(Script::Baal, &qs, "Sorceress", true));
    }

    #[test]
    fn test_duriel_requires_staff_pieces() {
        let mut qs = QuestState::default();
        qs.andariel = true;
        qs.level = 20;
        assert!(!should_run(Script::Duriel, &qs, "Sorceress", false));
        qs.amulet = true;
        qs.shaft = true;
        assert!(should_run(Script::Duriel, &qs, "Sorceress", false));
    }

    #[test]
    fn test_script_sequence_order() {
        // First script should be Den
        assert_eq!(SCRIPT_SEQUENCE[0], Script::Den);
        // Last script should be Baal
        assert_eq!(*SCRIPT_SEQUENCE.last().unwrap(), Script::Baal);
    }

    #[test]
    fn test_progression_engine_selects_den_first() {
        let dir = std::env::temp_dir().join("d2r_test_progression");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_quest_state.json");

        let mut engine = ProgressionEngine::new("Sorceress".into(), path.clone());
        engine.on_game_start();

        let next = engine.next_script();
        assert_eq!(next, Some(Script::Den));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_progression_engine_skips_to_bloodraven_after_den() {
        let dir = std::env::temp_dir().join("d2r_test_progression2");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_quest_state2.json");

        let mut engine = ProgressionEngine::new("Sorceress".into(), path.clone());
        engine.quest_state.den = true;
        engine.quest_state.level = 10;
        engine.on_game_start();

        // Den should be skipped (done), Bishibosh skipped (Sorceress),
        // next should be BloodRaven
        let next = engine.next_script();
        assert_eq!(next, Some(Script::BloodRaven));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_area_name_helpers() {
        assert_eq!(areas::town_act("Rogue Encampment"), Some(1));
        assert_eq!(areas::town_act("Lut Gholein"), Some(2));
        assert_eq!(areas::town_act("Blood Moor"), None);
        assert!(areas::is_town("Harrogath"));
        assert!(!areas::is_town("Chaos Sanctuary"));
    }

    #[test]
    fn test_quest_state_serialization() {
        let mut qs = QuestState::default();
        qs.den = true;
        qs.level = 15;
        qs.character_name = "TestSorc".into();

        let json = serde_json::to_string(&qs).unwrap();
        let loaded: QuestState = serde_json::from_str(&json).unwrap();
        assert!(loaded.den);
        assert_eq!(loaded.level, 15);
        assert_eq!(loaded.character_name, "TestSorc");
    }

    #[test]
    fn test_den_plan_has_level_gate() {
        let qs = QuestState::default();
        let plan = script_plan(Script::Den, &qs);
        assert!(matches!(plan[0], ScriptStep::RequireLevel { min_level: 8 }));
    }

    #[test]
    fn test_tristram_plan_includes_scroll_fetch() {
        let qs = QuestState::default();
        let plan = script_plan(Script::Tristram, &qs);
        // Should include interacting with Tree of Inifuss
        let has_tree = plan.iter().any(|s| matches!(s, ScriptStep::InteractObject { name } if *name == "Tree of Inifuss"));
        assert!(has_tree, "Tristram plan should include Tree of Inifuss interaction");
    }
}
