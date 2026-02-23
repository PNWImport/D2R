use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Complete agent configuration. Loaded from YAML, hot-reloadable.
/// Full port of kolbot Config + AutoSkill + AutoStat + Cubing + Runewords + Scripts.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    pub character_class: String,
    #[serde(default)]
    pub build: String,
    pub survival: SurvivalConfig,
    pub combat: CombatConfig,
    pub loot: LootConfig,
    pub town: TownConfig,
    pub buffs: Vec<BuffConfig>,
    pub humanization: HumanizationConfig,
    pub session: SessionConfig,
    #[serde(default)]
    pub farming: FarmingConfig,
    #[serde(default)]
    pub leveling: LevelingConfig,
    #[serde(default)]
    pub cubing: CubingConfig,
    #[serde(default)]
    pub runewords: RunewordConfig,
    #[serde(default)]
    pub gambling: GamblingConfig,
    #[serde(default)]
    pub class_specific: ClassSpecificConfig,
    #[serde(default)]
    pub monster_skip: MonsterSkipConfig,
    #[serde(default)]
    pub clear: ClearConfig,
    #[serde(default)]
    pub merc: MercConfig,
    #[serde(default)]
    pub inventory: InventoryConfig,
}

// ═══════════════════════════════════════════════════════════════
// SURVIVAL — kolbot Config.LifeChicken / UseHP / UseMP etc.
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SurvivalConfig {
    pub hp_potion_pct: u8,
    pub hp_rejuv_pct: u8,
    pub mana_potion_pct: u8,
    pub mana_rejuv_pct: u8,
    pub chicken_hp_pct: u8,
    pub mana_chicken_pct: u8,
    pub merc_chicken_pct: u8,
    pub tp_retreat_pct: u8,
    pub town_hp_pct: u8,
    pub town_mp_pct: u8,
    pub merc_hp_pct: u8,
    pub merc_rejuv_pct: u8,
    pub hp_potion_cooldown_ms: u64,
    pub mana_potion_cooldown_ms: u64,
    pub rejuv_cooldown_ms: u64,
    pub min_belt_column: [u8; 4],
    pub belt_column: Vec<String>, // kolbot: Config.BeltColumn = ["hp","hp","mp","rv"]
    pub hp_buffer: u8,            // Config.HPBuffer
    pub mp_buffer: u8,            // Config.MPBuffer
    pub rejuv_buffer: u8,         // Config.RejuvBuffer
}

impl Default for SurvivalConfig {
    fn default() -> Self {
        Self {
            hp_potion_pct: 75,
            hp_rejuv_pct: 40,
            mana_potion_pct: 30,
            mana_rejuv_pct: 0,
            chicken_hp_pct: 30,
            mana_chicken_pct: 0,
            merc_chicken_pct: 0,
            tp_retreat_pct: 35,
            town_hp_pct: 0,
            town_mp_pct: 0,
            merc_hp_pct: 75,
            merc_rejuv_pct: 0,
            hp_potion_cooldown_ms: 1000,
            mana_potion_cooldown_ms: 1000,
            rejuv_cooldown_ms: 300,
            min_belt_column: [3, 3, 3, 0],
            belt_column: vec!["hp".into(), "hp".into(), "mp".into(), "rv".into()],
            hp_buffer: 0,
            mp_buffer: 0,
            rejuv_buffer: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// COMBAT — kolbot Config.AttackSkill[0-6] + LowManaSkill + more
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CombatConfig {
    pub primary_skill_key: char,
    pub secondary_skill_key: char,
    pub mobility_skill_key: char,
    pub preattack_key: Option<char>,
    pub immunity_fallback_key: Option<char>,
    pub boss_priority: bool,
    pub static_field: Option<StaticFieldConfig>,
    pub kite_threshold: u8,
    pub cast_interval_ms: u64,
    pub max_attack_count: u32,
    pub use_merc_stomp: bool,
    // Full kolbot attack skill slots (hotkey equivalents)
    #[serde(default)]
    pub attack_slots: AttackSlots,
    #[serde(default)]
    pub low_mana_skill_key: Option<char>, // Config.LowManaSkill
    #[serde(default)]
    pub dodge: bool, // Config.Dodge
    #[serde(default)]
    pub dodge_range: u16, // Config.DodgeRange
    #[serde(default)]
    pub dodge_hp: u8, // Config.DodgeHP
    #[serde(default)]
    pub tele_stomp: bool, // Config.TeleStomp
    #[serde(default)]
    pub no_tele: bool, // Config.NoTele
    #[serde(default)]
    pub tele_switch: bool, // Config.TeleSwitch
    #[serde(default)]
    pub mf_switch_pct: u8, // Config.MFSwitchPercent
    #[serde(default)]
    pub wereform: Option<String>, // Config.Wereform
    #[serde(default)]
    pub custom_attack: HashMap<String, [Option<char>; 2]>, // Config.CustomAttack
}

/// All 7 attack skill slots from kolbot, mapped to hotkeys
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AttackSlots {
    pub preattack: Option<char>,      // AttackSkill[0]
    pub boss_primary: Option<char>,   // AttackSkill[1]
    pub boss_untimed: Option<char>,   // AttackSkill[2]
    pub mob_primary: Option<char>,    // AttackSkill[3]
    pub mob_untimed: Option<char>,    // AttackSkill[4]
    pub immune_primary: Option<char>, // AttackSkill[5]
    pub immune_untimed: Option<char>, // AttackSkill[6]
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StaticFieldConfig {
    pub hotkey: char,
    pub until_hp_pct: u8,
    pub max_casts: u8,
}

impl Default for CombatConfig {
    fn default() -> Self {
        Self {
            primary_skill_key: 'f',
            secondary_skill_key: 'g',
            mobility_skill_key: 'a',
            preattack_key: None,
            immunity_fallback_key: None,
            boss_priority: false,
            static_field: None,
            kite_threshold: 6,
            cast_interval_ms: 200,
            max_attack_count: 300,
            use_merc_stomp: true,
            attack_slots: AttackSlots::default(),
            low_mana_skill_key: None,
            dodge: false,
            dodge_range: 15,
            dodge_hp: 100,
            tele_stomp: false,
            no_tele: false,
            tele_switch: false,
            mf_switch_pct: 0,
            wereform: None,
            custom_attack: HashMap::new(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// LOOT — kolbot Pickit + Item settings
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LootConfig {
    pub always_pick: Vec<String>,
    pub conditional_pick: Vec<String>,
    pub magic_pick_types: Vec<String>,
    pub gold_threshold: u32,
    pub prioritize_runes_uniques: bool,
    pub max_pickup_distance: u16,
    pub keyword_always_pick: Vec<String>,
    #[serde(default)]
    pub pick_range: u16, // Config.PickRange
    #[serde(default)]
    pub fast_pick: bool, // Config.FastPick
    #[serde(default)]
    pub open_chests: bool, // Config.OpenChests.Enabled
    #[serde(default)]
    pub chest_range: u16, // Config.OpenChests.Range
    #[serde(default)]
    pub chest_types: Vec<String>, // Config.OpenChests.Types
    #[serde(default)]
    pub field_id: bool, // Config.FieldID.Enabled
    #[serde(default)]
    pub cain_id: bool, // Config.CainID.Enable
    #[serde(default)]
    pub skip_immunities: Vec<String>, // Config.SkipImmune
    #[serde(default)]
    pub stash_gold: u32, // Config.StashGold
}

impl Default for LootConfig {
    fn default() -> Self {
        Self {
            always_pick: vec!["unique".into(), "set".into(), "rune".into()],
            conditional_pick: vec!["rare_ring".into(), "rare_amulet".into()],
            magic_pick_types: vec!["grand_charm".into(), "small_charm".into()],
            gold_threshold: 5000,
            prioritize_runes_uniques: true,
            max_pickup_distance: 400,
            keyword_always_pick: vec![],
            pick_range: 40,
            fast_pick: false,
            open_chests: false,
            chest_range: 15,
            chest_types: vec!["chest".into(), "chest3".into()],
            field_id: false,
            cain_id: false,
            skip_immunities: vec![],
            stash_gold: 100000,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// TOWN — kolbot Town settings
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TownConfig {
    pub go_to_town_triggers: TownTriggers,
    pub task_order: Vec<String>,
    pub stash_rules: StashRules,
    #[serde(default)]
    pub heal_hp_pct: u8, // Config.HealHP
    #[serde(default)]
    pub heal_mp_pct: u8, // Config.HealMP
    #[serde(default)]
    pub heal_status: bool, // Config.HealStatus
    #[serde(default)]
    pub repair_pct: u8, // Config.RepairPercent
    #[serde(default)]
    pub cube_repair: bool, // Config.CubeRepair
    #[serde(default)]
    pub mini_shop_bot: bool, // Config.MiniShopBot
    #[serde(default)]
    pub town_check: bool, // Config.TownCheck
}

impl Default for TownConfig {
    fn default() -> Self {
        Self {
            go_to_town_triggers: TownTriggers::default(),
            task_order: vec![
                "revive_merc".into(),
                "heal".into(),
                "repair".into(),
                "identify".into(),
                "stash".into(),
                "buy_potions".into(),
            ],
            stash_rules: StashRules::default(),
            heal_hp_pct: 50,
            heal_mp_pct: 0,
            heal_status: false,
            repair_pct: 40,
            cube_repair: false,
            mini_shop_bot: true,
            town_check: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TownTriggers {
    pub belt_potions_below: u8,
    pub inventory_slots_below: u8,
    pub merc_dead: bool,
    pub durability_low: bool,
}

impl Default for TownTriggers {
    fn default() -> Self {
        Self {
            belt_potions_below: 2,
            inventory_slots_below: 4,
            merc_dead: true,
            durability_low: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StashRules {
    pub always_stash: Vec<String>,
    pub id_then_evaluate: Vec<String>,
    pub always_sell: Vec<String>,
}

impl Default for StashRules {
    fn default() -> Self {
        Self {
            always_stash: vec!["unique".into(), "set".into(), "rune".into()],
            id_then_evaluate: vec!["rare_ring".into(), "rare_amulet".into()],
            always_sell: vec!["magic_non_charm".into()],
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// BUFFS — kolbot Precast system
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuffConfig {
    pub name: String,
    pub hotkey: char,
    pub visual_check: bool,
    pub duration_secs: u32,
}

// ═══════════════════════════════════════════════════════════════
// FARMING — kolbot Scripts.* sequence (which runs to execute)
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct FarmingConfig {
    /// Ordered list of farming runs. Each run is an area/boss name.
    /// Agent executes in order, loops back to start.
    /// Names match kolbot script names: "Mephisto", "Baal", "Pit", etc.
    pub sequence: Vec<FarmRun>,
    #[serde(default)]
    pub min_game_time_secs: u32, // Config.MinGameTime
    #[serde(default)]
    pub max_game_time_mins: u32, // Config.MaxGameTime
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FarmRun {
    pub name: String, // Script name: "Mephisto", "Pit", "Baal", etc.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub options: HashMap<String, serde_yaml::Value>, // Per-script options
}

impl Default for FarmRun {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            options: HashMap::new(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// LEVELING — kolbot AutoSkill + AutoStat
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LevelingConfig {
    pub auto_skill: AutoSkillConfig,
    pub auto_stat: AutoStatConfig,
}

/// AutoSkill — spend skill points in order
/// Format matches kolbot: [[skill_name, max_points, satisfy], ...]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AutoSkillConfig {
    pub enabled: bool,
    #[serde(default)]
    pub save: u8, // Points to save unspent
    pub build: Vec<SkillAllocation>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillAllocation {
    pub skill: String,        // Skill name (e.g. "Blizzard", "Frozen Orb")
    pub hotkey: Option<char>, // Which hotkey this skill is bound to in-game
    pub max_points: u8,       // Max points to put in this skill
    #[serde(default = "default_true")]
    pub satisfy: bool, // Wait until this is done before moving to next
}

fn default_true() -> bool {
    true
}

/// AutoStat — spend stat points in order
/// Format matches kolbot: [[stat_type, target], ...]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AutoStatConfig {
    pub enabled: bool,
    #[serde(default)]
    pub save: u8,
    #[serde(default)]
    pub block_chance: u8, // Config.AutoStat.BlockChance
    pub build: Vec<StatAllocation>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StatAllocation {
    pub stat: String,       // "strength", "dexterity", "vitality", "energy"
    pub target: StatTarget, // Integer target or "all" or "block"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StatTarget {
    Value(u16),
    Keyword(String), // "all" or "block"
}

// ═══════════════════════════════════════════════════════════════
// CUBING — kolbot Config.Recipes
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CubingConfig {
    pub enabled: bool,
    pub recipes: Vec<CubeRecipe>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CubeRecipe {
    pub recipe_type: String, // "gem", "rune", "token", "reroll_magic", "socket", "upgrade", "craft"
    pub item: Option<String>, // Target item name
    #[serde(default)]
    pub ethereal: Option<String>, // "eth", "noneth", "all"
}

// ═══════════════════════════════════════════════════════════════
// RUNEWORDS — kolbot Config.Runewords
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RunewordConfig {
    pub enabled: bool,
    pub make: Vec<RunewordEntry>,
    #[serde(default)]
    pub keep_rules: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RunewordEntry {
    pub name: String, // "Insight", "Spirit", "Grief", "CTA", etc.
    pub base: String, // "Thresher", "Monarch", "Phase Blade", etc.
    #[serde(default)]
    pub ethereal: Option<String>, // "eth", "noneth", "all"
}

// ═══════════════════════════════════════════════════════════════
// GAMBLING — kolbot Config.Gamble*
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GamblingConfig {
    pub enabled: bool,
    #[serde(default)]
    pub gold_start: u32, // Config.GambleGoldStart
    #[serde(default)]
    pub gold_stop: u32, // Config.GambleGoldStop
    #[serde(default)]
    pub items: Vec<String>, // Config.GambleItems: ["Amulet", "Ring", "Circlet"]
}

// ═══════════════════════════════════════════════════════════════
// CLASS-SPECIFIC — kolbot per-class settings
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ClassSpecificConfig {
    // Amazon
    #[serde(default)]
    pub lightning_fury_delay: u8, // Config.LightningFuryDelay
    #[serde(default)]
    pub use_inner_sight: bool, // Config.UseInnerSight
    #[serde(default)]
    pub use_slow_missiles: bool, // Config.UseSlowMissiles
    #[serde(default)]
    pub use_decoy: bool, // Config.UseDecoy
    #[serde(default)]
    pub summon_valkyrie: bool, // Config.SummonValkyrie

    // Assassin
    #[serde(default)]
    pub use_traps: bool, // Config.UseTraps
    #[serde(default)]
    pub trap_keys: Vec<char>, // Trap skill hotkeys (5 slots)
    #[serde(default)]
    pub boss_trap_keys: Vec<char>, // Boss trap hotkeys
    #[serde(default)]
    pub summon_shadow: String, // "None", "Warrior", "Master"
    #[serde(default)]
    pub use_fade: bool, // Config.UseFade
    #[serde(default)]
    pub use_bos: bool, // Config.UseBoS
    #[serde(default)]
    pub use_venom: bool, // Config.UseVenom
    #[serde(default)]
    pub use_blade_shield: bool, // Config.UseBladeShield
    #[serde(default)]
    pub use_cloak: bool, // Config.UseCloakofShadows

    // Barbarian
    #[serde(default)]
    pub find_item: bool, // Config.FindItem
    #[serde(default)]
    pub find_item_switch: bool, // Config.FindItemSwitch
    #[serde(default)]
    pub use_warcries: bool, // Config.UseWarcries

    // Druid
    #[serde(default)]
    pub summon_raven: bool, // Config.SummonRaven
    #[serde(default)]
    pub summon_animal: String, // "None", "Spirit Wolf", "Dire Wolf", "Grizzly"
    #[serde(default)]
    pub summon_spirit: String, // "None", "Oak Sage", "Heart of Wolverine", "Spirit of Barbs"
    #[serde(default)]
    pub summon_vine: String, // "None", "Poison Creeper", "Carrion Vine", "Solar Creeper"

    // Necromancer
    #[serde(default)]
    pub boss_curse_key: Option<char>, // Config.Curse[0]
    #[serde(default)]
    pub mob_curse_key: Option<char>, // Config.Curse[1]
    #[serde(default)]
    pub explode_corpses_key: Option<char>, // Config.ExplodeCorpses
    #[serde(default)]
    pub golem: String, // "None", "Clay", "Blood", "Fire"
    #[serde(default)]
    pub skeletons: u8, // Config.Skeletons (0 = disabled)
    #[serde(default)]
    pub skeleton_mages: u8, // Config.SkeletonMages
    #[serde(default)]
    pub revives: u8, // Config.Revives
    #[serde(default)]
    pub active_summon: bool, // Config.ActiveSummon
    #[serde(default)]
    pub poison_nova_delay: u8, // Config.PoisonNovaDelay

    // Paladin
    #[serde(default)]
    pub vigor: bool, // Config.Vigor
    #[serde(default)]
    pub charge: bool, // Config.Charge
    #[serde(default)]
    pub redemption: Option<[u8; 2]>, // Config.Redemption = [life%, mana%]
    #[serde(default)]
    pub avoid_dolls: bool, // Config.AvoidDolls
    #[serde(default)]
    pub running_aura_key: Option<char>, // Config.RunningAura

    // Sorceress
    #[serde(default)]
    pub cast_static: u8, // Config.CastStatic (pct, 100 = disabled)
    #[serde(default)]
    pub static_list: Vec<String>, // Config.StaticList — monster names to static
    #[serde(default)]
    pub use_telekinesis: bool, // Config.UseTelekinesis
    #[serde(default)]
    pub use_energy_shield: bool, // Config.UseEnergyShield
    #[serde(default)]
    pub use_cold_armor: bool, // Config.UseColdArmor
}

// ═══════════════════════════════════════════════════════════════
// MONSTER SKIP — kolbot Config.SkipImmune / SkipEnchant / SkipAura
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct MonsterSkipConfig {
    pub skip_immune: Vec<String>, // ["fire", "cold", "lightning", "poison", "physical", "magic"]
    pub skip_enchant: Vec<String>, // ["extra strong", "cursed", "mana burn", etc.]
    pub skip_aura: Vec<String>,   // ["fanaticism", "might", "holy freeze", etc.]
    pub skip_exception: Vec<String>, // Always kill these despite skips
}

// ═══════════════════════════════════════════════════════════════
// CLEAR — kolbot Config.ClearType / ClearPath
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClearConfig {
    pub clear_type: String, // "all", "champions", "bosses", "skip_normal"
    pub clear_path: String, // Same as clear_type but while moving
    pub clear_range: u16,
}

impl Default for ClearConfig {
    fn default() -> Self {
        Self {
            clear_type: "skip_normal".into(),
            clear_path: "skip_normal".into(),
            clear_range: 30,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// MERC — kolbot Config.UseMerc / MercWatch
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MercConfig {
    pub use_merc: bool,
    pub merc_watch: bool, // Instant revive during battle
}

impl Default for MercConfig {
    fn default() -> Self {
        Self {
            use_merc: true,
            merc_watch: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// INVENTORY — kolbot Config.Inventory lock grid
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct InventoryConfig {
    /// 4x10 grid. 0 = locked (keep item), 1 = unlocked (can sell/stash/drop)
    pub grid: [[u8; 10]; 4],
}

// ═══════════════════════════════════════════════════════════════
// HUMANIZATION
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct HumanizationConfig {
    pub reaction_mean_ms: f32,
    pub reaction_stddev_ms: f32,
    pub survival_max_delay_ms: u32,
    pub potion_threshold_variance: u8,
    pub potion_forget_rate: f32,
    pub skill_miss_rate: f32,
    pub aim_variance_px: u16,
    pub path_deviate_rate: f32,
    pub path_deviate_px: u16,
    pub idle_pause_min_ms: u32,
    pub idle_pause_max_ms: u32,
    pub idle_pause_rate: f32,
    pub aggression_drift_per_hour: f32,
    pub caution_drift_per_hour: f32,
}

impl Default for HumanizationConfig {
    fn default() -> Self {
        Self {
            reaction_mean_ms: 280.0,
            reaction_stddev_ms: 90.0,
            survival_max_delay_ms: 150,
            potion_threshold_variance: 8,
            potion_forget_rate: 0.04,
            skill_miss_rate: 0.06,
            aim_variance_px: 15,
            path_deviate_rate: 0.08,
            path_deviate_px: 40,
            idle_pause_min_ms: 1500,
            idle_pause_max_ms: 6000,
            idle_pause_rate: 0.02,
            aggression_drift_per_hour: 0.04,
            caution_drift_per_hour: 0.03,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// SESSION
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SessionConfig {
    pub max_daily_hours: f32,
    pub session_min_minutes: u32,
    pub session_max_minutes: u32,
    pub break_min_minutes: u32,
    pub break_max_minutes: u32,
    pub allowed_start_hour: u8,
    pub allowed_end_hour: u8,
    pub day_off: Option<u8>,
    pub ladder_start_delay_hours: u32,
    pub short_break_rate: f32,
    pub long_break_rate: f32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_daily_hours: 8.0,
            session_min_minutes: 60,
            session_max_minutes: 180,
            break_min_minutes: 30,
            break_max_minutes: 120,
            allowed_start_hour: 9,
            allowed_end_hour: 23,
            day_off: Some(2),
            ladder_start_delay_hours: 48,
            short_break_rate: 0.12,
            long_break_rate: 0.04,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// DEFAULTS + IO
// ═══════════════════════════════════════════════════════════════

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            character_class: "Sorceress".into(),
            build: "blizzard".into(),
            survival: SurvivalConfig::default(),
            combat: CombatConfig::default(),
            loot: LootConfig::default(),
            town: TownConfig::default(),
            buffs: vec![],
            humanization: HumanizationConfig::default(),
            session: SessionConfig::default(),
            farming: FarmingConfig::default(),
            leveling: LevelingConfig::default(),
            cubing: CubingConfig::default(),
            runewords: RunewordConfig::default(),
            gambling: GamblingConfig::default(),
            class_specific: ClassSpecificConfig::default(),
            monster_skip: MonsterSkipConfig::default(),
            clear: ClearConfig::default(),
            merc: MercConfig::default(),
            inventory: InventoryConfig::default(),
        }
    }
}

impl AgentConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let content = serde_yaml::to_string(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_serializes() {
        let config = AgentConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("Sorceress"));
        assert!(yaml.contains("hp_potion_pct"));

        // Round-trip
        let loaded: AgentConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(loaded.survival.hp_potion_pct, 75);
        assert_eq!(loaded.survival.chicken_hp_pct, 30);
        assert_eq!(loaded.humanization.reaction_mean_ms, 280.0);
    }

    #[test]
    fn test_kolbot_defaults_match() {
        let config = AgentConfig::default();
        assert_eq!(config.survival.hp_potion_pct, 75);
        assert_eq!(config.survival.hp_rejuv_pct, 40);
        assert_eq!(config.survival.mana_potion_pct, 30);
        assert_eq!(config.survival.chicken_hp_pct, 30);
        assert_eq!(config.survival.hp_potion_cooldown_ms, 1000);
        assert_eq!(config.survival.rejuv_cooldown_ms, 300);
        assert_eq!(config.survival.min_belt_column, [3, 3, 3, 0]);
    }

    #[test]
    fn test_new_sections_serialize() {
        let config = AgentConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        // Verify new sections are present
        assert!(yaml.contains("farming"));
        assert!(yaml.contains("leveling"));
        assert!(yaml.contains("cubing"));
        assert!(yaml.contains("runewords"));
        assert!(yaml.contains("gambling"));
        assert!(yaml.contains("class_specific"));
        assert!(yaml.contains("monster_skip"));
        assert!(yaml.contains("clear"));
        assert!(yaml.contains("merc"));
        assert!(yaml.contains("inventory"));
    }
}
