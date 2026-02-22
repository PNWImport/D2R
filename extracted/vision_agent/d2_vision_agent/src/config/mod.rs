use serde::{Deserialize, Serialize};
use std::path::Path;

/// Complete agent configuration. Loaded from YAML, hot-reloadable.
/// Combines distilled kolbot logic with humanization parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    pub character_class: String,
    pub survival: SurvivalConfig,
    pub combat: CombatConfig,
    pub loot: LootConfig,
    pub town: TownConfig,
    pub buffs: Vec<BuffConfig>,
    pub humanization: HumanizationConfig,
    pub session: SessionConfig,
}

/// Survival thresholds — kolbot Config values
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurvivalConfig {
    pub hp_potion_pct: u8,
    pub hp_rejuv_pct: u8,
    pub mana_potion_pct: u8,
    pub chicken_hp_pct: u8,
    pub tp_retreat_pct: u8,
    pub town_hp_pct: u8,
    pub hp_potion_cooldown_ms: u64,
    pub mana_potion_cooldown_ms: u64,
    pub rejuv_cooldown_ms: u64,
    pub min_belt_column: [u8; 4],
}

impl Default for SurvivalConfig {
    fn default() -> Self {
        Self {
            hp_potion_pct: 75,
            hp_rejuv_pct: 40,
            mana_potion_pct: 30,
            chicken_hp_pct: 30,
            tp_retreat_pct: 35,
            town_hp_pct: 0,
            hp_potion_cooldown_ms: 1000,
            mana_potion_cooldown_ms: 1000,
            rejuv_cooldown_ms: 300,
            min_belt_column: [3, 3, 3, 0],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LootConfig {
    pub always_pick: Vec<String>,
    pub conditional_pick: Vec<String>,
    pub magic_pick_types: Vec<String>,
    pub gold_threshold: u32,
    pub prioritize_runes_uniques: bool,
    pub max_pickup_distance: u16,
    pub keyword_always_pick: Vec<String>,
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
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TownConfig {
    pub go_to_town_triggers: TownTriggers,
    pub task_order: Vec<String>,
    pub stash_rules: StashRules,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuffConfig {
    pub name: String,
    pub hotkey: char,
    pub visual_check: bool,
    pub duration_secs: u32,
}

/// Humanization — integrated into decision engine, not a wrapper.
#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            character_class: "Sorceress".into(),
            survival: SurvivalConfig::default(),
            combat: CombatConfig::default(),
            loot: LootConfig::default(),
            town: TownConfig::default(),
            buffs: vec![],
            humanization: HumanizationConfig::default(),
            session: SessionConfig::default(),
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
        // These should match what our distiller extracted
        assert_eq!(config.survival.hp_potion_pct, 75);    // kolbot: Config.UseHP = 75
        assert_eq!(config.survival.hp_rejuv_pct, 40);     // kolbot: Config.UseRejuvHP = 40
        assert_eq!(config.survival.mana_potion_pct, 30);  // kolbot: Config.UseMP = 30
        assert_eq!(config.survival.chicken_hp_pct, 30);   // kolbot: Config.LifeChicken = 30
        assert_eq!(config.survival.hp_potion_cooldown_ms, 1000);  // kolbot: 1000ms
        assert_eq!(config.survival.rejuv_cooldown_ms, 300);       // kolbot: 300ms
        assert_eq!(config.survival.min_belt_column, [3, 3, 3, 0]); // kolbot: MinColumn
    }
}
