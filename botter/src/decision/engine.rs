use crate::config::*;
use crate::vision::{FrameState, ItemQuality};
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use std::time::{Duration, Instant};

/// Action the agent should execute
#[derive(Debug, Clone)]
pub enum Action {
    DrinkPotion { belt_slot: u8 },
    CastSkill { key: char, screen_x: i32, screen_y: i32 },
    PickupLoot { screen_x: i32, screen_y: i32 },
    MoveTo { screen_x: i32, screen_y: i32 },
    TownPortal,
    ChickenQuit,
    RecastBuff { key: char },
    TakeBreak { duration: Duration },
    IdlePause { duration: Duration },
    Dodge { screen_x: i32, screen_y: i32 },
    SwitchWeapon,
    Wait,
}

/// Monster context passed from vision layer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetType {
    Boss,
    Champion,
    Normal,
    Immune,
}

/// Decision with attached humanized delay
#[derive(Debug, Clone)]
pub struct Decision {
    pub action: Action,
    pub delay: Duration,
    pub priority: u8,
    pub reason: &'static str,
}

pub struct DecisionEngine {
    config: AgentConfig,
    rng: StdRng,

    // Timing state
    pub(crate) last_hp_potion: Instant,
    pub(crate) last_mana_potion: Instant,
    pub(crate) last_rejuv: Instant,
    pub(crate) last_attack: Instant,
    pub(crate) last_break_check: Instant,
    pub(crate) last_static_field: Instant,
    pub(crate) last_preattack: Instant,
    session_start: Instant,

    // Humanization state
    current_aggression: f32,
    current_caution: f32,
    action_count: u64,
    reaction_dist: Normal<f64>,

    // Intentional mistake tracking
    kills_since_last_mistake: u32,
    next_mistake_at: u32,

    // Combat state tracking
    static_field_casts: u8,
    on_weapon_switch: bool,
}

impl DecisionEngine {
    pub fn new(config: AgentConfig) -> Self {
        let reaction_dist = Normal::new(
            config.humanization.reaction_mean_ms as f64,
            config.humanization.reaction_stddev_ms as f64,
        )
        .unwrap_or_else(|_| Normal::new(280.0, 90.0).unwrap());

        let mut rng = StdRng::from_entropy();
        let next_mistake = rng.gen_range(20..100);

        let now = Instant::now();
        Self {
            config,
            rng,
            last_hp_potion: now,
            last_mana_potion: now,
            last_rejuv: now,
            last_attack: now,
            last_break_check: now,
            last_static_field: now,
            last_preattack: now,
            session_start: now,
            current_aggression: 0.7,
            current_caution: 0.5,
            action_count: 0,
            reaction_dist,
            kills_since_last_mistake: 0,
            next_mistake_at: next_mistake,
            static_field_casts: 0,
            on_weapon_switch: false,
        }
    }

    /// Main decision function. Returns a Decision with action + humanized delay.
    pub fn decide(&mut self, state: &FrameState) -> Decision {
        self.action_count += 1;
        self.update_fatigue_drift();

        // Priority 0: Session break check (infrequent)
        if let Some(d) = self.check_break() {
            return d;
        }

        // Priority 1: CHICKEN (HP)
        if state.hp_pct <= self.config.survival.chicken_hp_pct && state.in_combat {
            return Decision {
                action: Action::ChickenQuit,
                delay: Duration::ZERO,
                priority: 0,
                reason: "chicken: HP critical",
            };
        }

        // Priority 1b: CHICKEN (Mana — kolbot Config.ManaChicken)
        if let Some(d) = self.check_mana_chicken(state) {
            return d;
        }

        // Priority 1c: CHICKEN (Merc — kolbot Config.MercChicken)
        if let Some(d) = self.check_merc_chicken(state) {
            return d;
        }

        // Priority 2: REJUV
        if let Some(d) = self.check_rejuv(state) {
            return d;
        }

        // Priority 3: HP POTION
        if let Some(d) = self.check_hp_potion(state) {
            return d;
        }

        // Priority 4: TP RETREAT
        if state.hp_pct <= self.config.survival.tp_retreat_pct && state.in_combat {
            return Decision {
                action: Action::TownPortal,
                delay: self.survival_delay(),
                priority: 4,
                reason: "retreat: HP dangerously low",
            };
        }

        // Priority 4b: DODGE (kolbot Config.Dodge)
        if let Some(d) = self.check_dodge(state) {
            return d;
        }

        // Priority 5: MANA POTION
        if let Some(d) = self.check_mana_potion(state) {
            return d;
        }

        // Priority 6: BUFF RECAST
        if let Some(d) = self.check_buffs(state) {
            return d;
        }

        // Priority 7: INTENTIONAL MISTAKE (humanization)
        if let Some(d) = self.check_intentional_mistake(state) {
            return d;
        }

        // Priority 7b: STATIC FIELD (Sorceress — before main attack)
        if let Some(d) = self.check_static_field(state) {
            return d;
        }

        // Priority 7c: PREATTACK (kolbot AttackSkill[0] — Hurricane, Battle Cry, etc.)
        if let Some(d) = self.check_preattack(state) {
            return d;
        }

        // Priority 7d: MF WEAPON SWITCH (kolbot Config.MFSwitchPercent — swap before kill)
        if let Some(d) = self.check_mf_switch(state) {
            return d;
        }

        // Priority 8: ATTACK (uses full attack_slots system)
        if let Some(d) = self.check_attack(state) {
            return d;
        }

        // Priority 9: LOOT
        if let Some(d) = self.check_loot(state) {
            return d;
        }

        // Priority 10: IDLE PAUSE
        if let Some(d) = self.check_idle_pause() {
            return d;
        }

        // Priority 11: NAVIGATE
        self.navigate(state)
    }

    // --- Survival ---

    fn check_rejuv(&mut self, state: &FrameState) -> Option<Decision> {
        let now = Instant::now();
        if now.duration_since(self.last_rejuv)
            < Duration::from_millis(self.config.survival.rejuv_cooldown_ms)
        {
            return None;
        }

        let threshold = self.humanize_threshold(self.config.survival.hp_rejuv_pct);

        if state.hp_pct <= threshold {
            if state.hp_pct > 25
                && self.rng.gen::<f32>() < self.config.humanization.potion_forget_rate
            {
                return None; // "forgot"
            }
            self.last_rejuv = now;
            return Some(Decision {
                action: Action::DrinkPotion { belt_slot: 3 },
                delay: self.survival_delay(),
                priority: 2,
                reason: "rejuv: HP below panic threshold",
            });
        }
        None
    }

    fn check_hp_potion(&mut self, state: &FrameState) -> Option<Decision> {
        let now = Instant::now();
        if now.duration_since(self.last_hp_potion)
            < Duration::from_millis(self.config.survival.hp_potion_cooldown_ms)
        {
            return None;
        }

        let threshold = self.humanize_threshold(self.config.survival.hp_potion_pct);

        if state.hp_pct <= threshold {
            if state.hp_pct > self.config.survival.hp_rejuv_pct
                && self.rng.gen::<f32>() < self.config.humanization.potion_forget_rate
            {
                return None;
            }
            self.last_hp_potion = now;
            return Some(Decision {
                action: Action::DrinkPotion { belt_slot: 0 },
                delay: self.survival_delay(),
                priority: 3,
                reason: "hp potion: below threshold",
            });
        }
        None
    }

    fn check_mana_potion(&mut self, state: &FrameState) -> Option<Decision> {
        let now = Instant::now();
        if now.duration_since(self.last_mana_potion)
            < Duration::from_millis(self.config.survival.mana_potion_cooldown_ms)
        {
            return None;
        }

        let threshold = self.humanize_threshold(self.config.survival.mana_potion_pct);

        if state.mana_pct <= threshold && !state.in_town {
            self.last_mana_potion = now;
            return Some(Decision {
                action: Action::DrinkPotion { belt_slot: 2 },
                delay: self.normal_delay(),
                priority: 5,
                reason: "mana potion: below threshold",
            });
        }
        None
    }

    // --- Mana / Merc Chicken (kolbot Config.ManaChicken, Config.MercChicken) ---

    fn check_mana_chicken(&self, state: &FrameState) -> Option<Decision> {
        let threshold = self.config.survival.mana_chicken_pct;
        if threshold == 0 || state.in_town {
            return None;
        }
        if state.mana_pct <= threshold && state.in_combat {
            return Some(Decision {
                action: Action::ChickenQuit,
                delay: Duration::ZERO,
                priority: 0,
                reason: "chicken: mana critical",
            });
        }
        None
    }

    fn check_merc_chicken(&self, state: &FrameState) -> Option<Decision> {
        let threshold = self.config.survival.merc_chicken_pct;
        if threshold == 0 || state.in_town || !self.config.merc.use_merc {
            return None;
        }
        // Merc chicken: leave game if merc HP falls below threshold
        if state.merc_alive && state.merc_hp_pct <= threshold && state.in_combat {
            return Some(Decision {
                action: Action::ChickenQuit,
                delay: Duration::ZERO,
                priority: 0,
                reason: "chicken: merc HP critical",
            });
        }
        None
    }

    // --- Dodge (kolbot Config.Dodge) ---

    fn check_dodge(&mut self, state: &FrameState) -> Option<Decision> {
        if !self.config.combat.dodge || state.in_town || !state.in_combat {
            return None;
        }

        // Only dodge when HP is below dodge threshold
        if state.hp_pct > self.config.combat.dodge_hp {
            return None;
        }

        // Dodge when enemies are very close (within kite threshold)
        if state.enemy_count < self.config.combat.kite_threshold {
            return None;
        }

        // Move away from the nearest enemy
        let dx = state.char_screen_x as i32 - state.nearest_enemy_x as i32;
        let dy = state.char_screen_y as i32 - state.nearest_enemy_y as i32;

        // Normalize and move in the opposite direction
        let dist = ((dx * dx + dy * dy) as f32).sqrt().max(1.0);
        let dodge_dist = self.config.combat.dodge_range as f32 * 10.0;
        let dodge_x = state.char_screen_x as i32 + (dx as f32 / dist * dodge_dist) as i32;
        let dodge_y = state.char_screen_y as i32 + (dy as f32 / dist * dodge_dist) as i32;

        let (dx, dy) = self.humanize_position(dodge_x, dodge_y);

        Some(Decision {
            action: Action::Dodge {
                screen_x: dx,
                screen_y: dy,
            },
            delay: self.survival_delay(),
            priority: 4,
            reason: "dodge: HP low, enemies close",
        })
    }

    // --- Static Field (Sorceress — kolbot Config.CastStatic) ---

    fn check_static_field(&mut self, state: &FrameState) -> Option<Decision> {
        let sf = match self.config.combat.static_field.as_ref() {
            Some(sf) => sf,
            None => return None,
        };

        if state.in_town || !state.in_combat || !state.boss_present {
            self.static_field_casts = 0;
            return None;
        }

        // Only cast if boss HP is above the threshold
        if state.nearest_enemy_hp_pct <= sf.until_hp_pct {
            return None;
        }

        // Max casts per engagement
        if self.static_field_casts >= sf.max_casts {
            return None;
        }

        let now = Instant::now();
        if now.duration_since(self.last_static_field) < Duration::from_millis(300) {
            return None;
        }

        // Copy hotkey before mutable borrow
        let hotkey = sf.hotkey;

        self.last_static_field = now;
        self.static_field_casts += 1;

        let (tx, ty) = self.humanize_position(
            state.nearest_enemy_x as i32,
            state.nearest_enemy_y as i32,
        );

        Some(Decision {
            action: Action::CastSkill {
                key: hotkey,
                screen_x: tx,
                screen_y: ty,
            },
            delay: self.attack_delay(),
            priority: 7,
            reason: "static field: boss HP above threshold",
        })
    }

    // --- Preattack (kolbot AttackSkill[0] — Hurricane, Battle Cry, etc.) ---

    fn check_preattack(&mut self, state: &FrameState) -> Option<Decision> {
        let preattack_key = self.config.combat.attack_slots.preattack
            .or(self.config.combat.preattack_key)?;

        if state.in_town || state.enemy_count == 0 {
            return None;
        }

        let now = Instant::now();
        // Only preattack every 10 seconds (kolbot re-casts warcries/auras periodically)
        if now.duration_since(self.last_preattack) < Duration::from_secs(10) {
            return None;
        }

        self.last_preattack = now;

        let (tx, ty) = self.humanize_position(
            state.nearest_enemy_x as i32,
            state.nearest_enemy_y as i32,
        );

        Some(Decision {
            action: Action::CastSkill {
                key: preattack_key,
                screen_x: tx,
                screen_y: ty,
            },
            delay: self.attack_delay(),
            priority: 7,
            reason: "preattack: warcry/debuff",
        })
    }

    // --- MF Weapon Switch (kolbot Config.MFSwitchPercent) ---

    fn check_mf_switch(&mut self, state: &FrameState) -> Option<Decision> {
        let mf_pct = self.config.combat.mf_switch_pct;
        if mf_pct == 0 {
            return None;
        }

        if state.in_town || state.enemy_count == 0 {
            // Switch back to main weapon if we're on MF switch
            if self.on_weapon_switch {
                self.on_weapon_switch = false;
                return Some(Decision {
                    action: Action::SwitchWeapon,
                    delay: self.normal_delay(),
                    priority: 8,
                    reason: "mf switch: back to main weapon",
                });
            }
            return None;
        }

        // Switch to MF weapon when boss/champion is low HP
        if (state.boss_present || state.champion_present)
            && state.nearest_enemy_hp_pct <= mf_pct
            && state.nearest_enemy_hp_pct > 0
            && !self.on_weapon_switch
        {
            self.on_weapon_switch = true;
            return Some(Decision {
                action: Action::SwitchWeapon,
                delay: self.attack_delay(),
                priority: 8,
                reason: "mf switch: boss low HP, swap for MF",
            });
        }

        None
    }

    // --- Combat ---

    /// Derive target type from vision state for attack slot selection
    fn derive_target_type(&self, state: &FrameState) -> TargetType {
        if state.immune_detected {
            TargetType::Immune
        } else if state.boss_present {
            TargetType::Boss
        } else if state.champion_present {
            TargetType::Champion
        } else {
            TargetType::Normal
        }
    }

    /// Select the right skill key based on target type using attack_slots.
    /// Falls back to primary/secondary keys if slots aren't configured.
    /// kolbot mapping:
    ///   AttackSkill[0] = preattack (handled separately)
    ///   AttackSkill[1] = boss timed (boss_primary)
    ///   AttackSkill[2] = boss untimed (boss_untimed)
    ///   AttackSkill[3] = mob timed (mob_primary)
    ///   AttackSkill[4] = mob untimed (mob_untimed)
    ///   AttackSkill[5] = immune timed (immune_primary)
    ///   AttackSkill[6] = immune untimed (immune_untimed)
    fn select_attack_key(&mut self, target: TargetType, timed: bool) -> char {
        let slots = &self.config.combat.attack_slots;

        let slot_key = match (target, timed) {
            (TargetType::Boss, true) | (TargetType::Champion, true) => slots.boss_primary,
            (TargetType::Boss, false) | (TargetType::Champion, false) => slots.boss_untimed,
            (TargetType::Immune, true) => slots.immune_primary,
            (TargetType::Immune, false) => slots.immune_untimed,
            (TargetType::Normal, true) => slots.mob_primary,
            (TargetType::Normal, false) => slots.mob_untimed,
        };

        // Fall back to configured primary/secondary if attack slot is empty
        slot_key.unwrap_or_else(|| {
            match target {
                TargetType::Immune => self.config.combat.immunity_fallback_key
                    .unwrap_or(self.config.combat.primary_skill_key),
                TargetType::Boss | TargetType::Champion => self.config.combat.primary_skill_key,
                TargetType::Normal => self.config.combat.primary_skill_key,
            }
        })
    }

    fn check_attack(&mut self, state: &FrameState) -> Option<Decision> {
        if state.enemy_count == 0 || state.in_town {
            // Reset static field counter when no enemies
            self.static_field_casts = 0;
            return None;
        }

        let now = Instant::now();
        if now.duration_since(self.last_attack)
            < Duration::from_millis(self.config.combat.cast_interval_ms)
        {
            return None;
        }

        self.last_attack = now;
        self.kills_since_last_mistake += 1;

        // Kite check (kolbot: same logic for overwhelming mobs)
        if state.enemy_count > self.config.combat.kite_threshold {
            let (kx, ky) = self.humanize_position(
                state.char_screen_x as i32,
                state.char_screen_y as i32 + 150,
            );
            return Some(Decision {
                action: Action::MoveTo {
                    screen_x: kx,
                    screen_y: ky,
                },
                delay: self.attack_delay(),
                priority: 7,
                reason: "kite: too many enemies",
            });
        }

        // Low mana fallback (kolbot Config.LowManaSkill)
        if let Some(low_mana_key) = self.config.combat.low_mana_skill_key {
            if state.mana_pct < 15 {
                let (tx, ty) = self.humanize_position(
                    state.nearest_enemy_x as i32,
                    state.nearest_enemy_y as i32,
                );
                return Some(Decision {
                    action: Action::CastSkill {
                        key: low_mana_key,
                        screen_x: tx,
                        screen_y: ty,
                    },
                    delay: self.attack_delay(),
                    priority: 8,
                    reason: "attack: low mana fallback",
                });
            }
        }

        // Derive target type from vision state
        let target = self.derive_target_type(state);

        // Select skill using attack_slots system
        // Alternate between timed (primary) and untimed skills for bosses
        let use_timed = if matches!(target, TargetType::Boss | TargetType::Champion) {
            // Alternate: 70% timed, 30% untimed (matches kolbot tick-based alternation)
            self.rng.gen::<f32>() < 0.7
        } else {
            // Mobs: mostly timed, occasionally untimed (Death Sentry for corpses, etc.)
            self.rng.gen::<f32>() < 0.8
        };

        let mut skill_key = self.select_attack_key(target, use_timed);

        // Humanization: occasional wrong skill press
        if self.rng.gen::<f32>() < self.config.humanization.skill_miss_rate {
            skill_key = self.config.combat.secondary_skill_key;
        }

        // Target nearest enemy position instead of fixed offset
        let (tx, ty) = self.humanize_position(
            state.nearest_enemy_x as i32,
            state.nearest_enemy_y as i32,
        );

        let reason = match target {
            TargetType::Boss => "attack: boss target",
            TargetType::Champion => "attack: champion target",
            TargetType::Immune => "attack: immune target (fallback)",
            TargetType::Normal => "attack: mob clear",
        };

        Some(Decision {
            action: Action::CastSkill {
                key: skill_key,
                screen_x: tx,
                screen_y: ty,
            },
            delay: self.attack_delay(),
            priority: 8,
            reason,
        })
    }

    // --- Loot (kolbot Pickit priority) ---

    fn check_loot(&self, state: &FrameState) -> Option<Decision> {
        if state.in_combat && state.enemy_count > 2 {
            return None;
        }
        if state.loot_label_count == 0 {
            return None;
        }

        // Find best loot target (kolbot: sortFastPickItems — runes/uniques first, then distance)
        let labels = &state.loot_labels[..state.loot_label_count as usize];

        let best = labels
            .iter()
            .filter(|l| {
                matches!(
                    l.quality,
                    ItemQuality::Unique
                        | ItemQuality::Set
                        | ItemQuality::Rune
                        | ItemQuality::Rare
                )
            })
            .min_by_key(|l| {
                // Priority: runes/uniques get negative distance bonus
                let priority_bonus: i32 = match l.quality {
                    ItemQuality::Rune | ItemQuality::Unique => -10000,
                    ItemQuality::Set => -5000,
                    _ => 0,
                };
                let dx = l.x as i32 - state.char_screen_x as i32;
                let dy = l.y as i32 - state.char_screen_y as i32;
                priority_bonus + dx * dx + dy * dy
            });

        best.map(|label| Decision {
            action: Action::PickupLoot {
                screen_x: label.x as i32,
                screen_y: label.y as i32,
            },
            delay: Duration::from_millis(0), // normal_delay applied by caller
            priority: 9,
            reason: "loot: valuable item detected",
        })
    }

    // --- Buffs ---

    fn check_buffs(&self, state: &FrameState) -> Option<Decision> {
        for (i, buff) in self.config.buffs.iter().enumerate() {
            if buff.visual_check {
                let bit = 1u16 << i;
                if state.active_buffs & bit == 0 {
                    return Some(Decision {
                        action: Action::RecastBuff { key: buff.hotkey },
                        delay: Duration::from_millis(0),
                        priority: 6,
                        reason: "buff: missing from UI",
                    });
                }
            }
        }
        None
    }

    // --- Humanization: intentional mistakes ---

    fn check_intentional_mistake(&mut self, state: &FrameState) -> Option<Decision> {
        if self.kills_since_last_mistake < self.next_mistake_at {
            return None;
        }

        self.kills_since_last_mistake = 0;
        self.next_mistake_at = self.rng.gen_range(20..100);

        let mistake_type = self.rng.gen_range(0..4);
        match mistake_type {
            0 if !state.in_town => {
                // Stand still for a few seconds (got distracted)
                let duration = Duration::from_millis(self.rng.gen_range(2000..5000));
                Some(Decision {
                    action: Action::IdlePause { duration },
                    delay: Duration::ZERO,
                    priority: 7,
                    reason: "mistake: idle stall",
                })
            }
            1 if !state.in_town => {
                // Move to a random spot briefly (wrong click)
                let offset_x = self.rng.gen_range(-200i32..200);
                let offset_y = self.rng.gen_range(-150i32..150);
                let (x, y) = self.humanize_position(
                    state.char_screen_x as i32 + offset_x,
                    state.char_screen_y as i32 + offset_y,
                );
                Some(Decision {
                    action: Action::MoveTo {
                        screen_x: x,
                        screen_y: y,
                    },
                    delay: Duration::from_millis(self.rng.gen_range(50..150)),
                    priority: 7,
                    reason: "mistake: wrong click",
                })
            }
            _ => None, // No mistake this time (some types only valid in certain contexts)
        }
    }

    // --- Navigation ---

    fn navigate(&mut self, state: &FrameState) -> Decision {
        if state.in_town {
            return Decision {
                action: Action::Wait,
                delay: Duration::from_millis(500),
                priority: 11,
                reason: "in town: awaiting strategy layer",
            };
        }

        let angle: f32 = self.rng.gen_range(0.0..std::f32::consts::TAU);
        let distance: f32 = self.rng.gen_range(100.0..250.0);

        let (tx, ty) = if self.rng.gen::<f32>() < self.config.humanization.path_deviate_rate {
            (
                state.char_screen_x as i32 + (angle.cos() * distance) as i32,
                state.char_screen_y as i32 + (angle.sin() * distance) as i32,
            )
        } else {
            (
                state.char_screen_x as i32 + self.rng.gen_range(-80..80),
                state.char_screen_y as i32 - self.rng.gen_range(80..200),
            )
        };

        Decision {
            action: Action::MoveTo {
                screen_x: tx,
                screen_y: ty,
            },
            delay: self.normal_delay(),
            priority: 11,
            reason: "navigate: exploring",
        }
    }

    // --- Humanization helpers ---

    fn humanize_threshold(&mut self, base: u8) -> u8 {
        let v = self.config.humanization.potion_threshold_variance as i8;
        let offset = self.rng.gen_range(-v..=v);
        (base as i16 + offset as i16).clamp(20, 95) as u8
    }

    fn humanize_position(&mut self, x: i32, y: i32) -> (i32, i32) {
        let v = self.config.humanization.aim_variance_px as i32;
        (
            x + self.rng.gen_range(-v..=v),
            y + self.rng.gen_range(-v..=v),
        )
    }

    pub fn survival_delay(&mut self) -> Duration {
        let sample = self.reaction_dist.sample(&mut self.rng).max(30.0) as u64;
        let capped = sample.min(self.config.humanization.survival_max_delay_ms as u64);
        Duration::from_millis(capped)
    }

    pub fn normal_delay(&mut self) -> Duration {
        let mut sample = self.reaction_dist.sample(&mut self.rng).max(50.0) as u64;
        if self.rng.gen::<f32>() < 0.03 {
            sample = (sample as f64 * 2.5) as u64;
        }
        Duration::from_millis(sample.min(1500))
    }

    pub fn attack_delay(&mut self) -> Duration {
        let sample = (self.reaction_dist.sample(&mut self.rng) * 0.6).max(20.0) as u64;
        Duration::from_millis(sample.min(400))
    }

    fn check_idle_pause(&mut self) -> Option<Decision> {
        if self.rng.gen::<f32>() < self.config.humanization.idle_pause_rate / 60.0 {
            let min = self.config.humanization.idle_pause_min_ms;
            let max = self.config.humanization.idle_pause_max_ms;
            let duration = Duration::from_millis(self.rng.gen_range(min..=max) as u64);
            return Some(Decision {
                action: Action::IdlePause { duration },
                delay: Duration::ZERO,
                priority: 10,
                reason: "idle: human pause",
            });
        }
        None
    }

    fn update_fatigue_drift(&mut self) {
        let hours = self.session_start.elapsed().as_secs_f32() / 3600.0;
        self.current_aggression =
            (0.7 + hours * self.config.humanization.aggression_drift_per_hour).min(1.0);
        self.current_caution =
            (0.5 - hours * self.config.humanization.caution_drift_per_hour).max(0.2);
    }

    fn check_break(&mut self) -> Option<Decision> {
        let now = Instant::now();
        if now.duration_since(self.last_break_check) < Duration::from_secs(300) {
            return None;
        }
        self.last_break_check = now;

        let session_hours = self.session_start.elapsed().as_secs_f32() / 3600.0;

        if session_hours >= self.config.session.max_daily_hours {
            let mins = self.rng.gen_range(
                self.config.session.break_min_minutes..=self.config.session.break_max_minutes,
            );
            return Some(Decision {
                action: Action::TakeBreak {
                    duration: Duration::from_secs(mins as u64 * 60),
                },
                delay: Duration::ZERO,
                priority: 0,
                reason: "break: session limit reached",
            });
        }

        if self.rng.gen::<f32>() < self.config.session.short_break_rate * 5.0 / 60.0 {
            let mins = self.rng.gen_range(2u64..8);
            return Some(Decision {
                action: Action::TakeBreak {
                    duration: Duration::from_secs(mins * 60),
                },
                delay: Duration::ZERO,
                priority: 0,
                reason: "break: random short break",
            });
        }

        None
    }

    /// Hot-reload config from YAML
    pub fn reload_config(&mut self, config: AgentConfig) {
        self.reaction_dist = Normal::new(
            config.humanization.reaction_mean_ms as f64,
            config.humanization.reaction_stddev_ms as f64,
        )
        .unwrap_or_else(|_| Normal::new(280.0, 90.0).unwrap());
        self.config = config;
    }

    pub fn session_elapsed(&self) -> Duration {
        self.session_start.elapsed()
    }

    pub fn action_count(&self) -> u64 {
        self.action_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::FrameState;

    fn make_engine() -> DecisionEngine {
        DecisionEngine::new(AgentConfig::default())
    }

    fn combat_state(hp: u8, mana: u8, enemies: u8) -> FrameState {
        let mut s = FrameState::default();
        s.hp_pct = hp;
        s.mana_pct = mana;
        s.enemy_count = enemies;
        s.in_combat = enemies > 0;
        s.in_town = false;
        // Set nearest enemy to a sensible position
        s.nearest_enemy_x = 400;
        s.nearest_enemy_y = 220;
        s.nearest_enemy_hp_pct = 100;
        s
    }

    #[test]
    fn test_chicken_at_low_hp() {
        let mut engine = make_engine();
        let state = combat_state(25, 50, 3);

        let d = engine.decide(&state);
        assert!(
            matches!(d.action, Action::ChickenQuit),
            "should chicken at 25% HP, got: {:?}",
            d.action
        );
        assert_eq!(d.delay, Duration::ZERO);
    }

    #[test]
    fn test_hp_potion_at_threshold() {
        let mut engine = make_engine();
        // HP at 60 — well below threshold of 75 +/- 8 variance
        let state = combat_state(60, 80, 2);

        // Run many times since humanization can "forget" or variance might shift threshold
        let mut found_potion = false;
        for _ in 0..50 {
            let d = engine.decide(&state);
            if matches!(d.action, Action::DrinkPotion { belt_slot: 0 }) {
                found_potion = true;
                break;
            }
            // Reset potion cooldown for next iteration
            engine.last_hp_potion = Instant::now() - Duration::from_secs(5);
        }
        assert!(found_potion, "should drink HP potion when HP is well below threshold");
    }

    #[test]
    fn test_rejuv_at_critical_hp() {
        let mut engine = make_engine();
        // HP at 32 — below rejuv threshold (40) and tp_retreat (35)
        // Also below chicken would be 30, so 32 is in the rejuv zone
        let state = combat_state(32, 80, 5);

        let mut found_rejuv = false;
        let mut found_tp = false;
        for _ in 0..50 {
            let d = engine.decide(&state);
            match &d.action {
                Action::DrinkPotion { belt_slot: 3 } => { found_rejuv = true; break; }
                Action::TownPortal => { found_tp = true; }
                _ => {}
            }
            // Reset cooldowns
            engine.last_rejuv = Instant::now() - Duration::from_secs(5);
            engine.last_hp_potion = Instant::now() - Duration::from_secs(5);
        }
        // Either rejuv or TP is acceptable at 32% HP (both are valid survival responses)
        assert!(
            found_rejuv || found_tp,
            "should rejuv or TP at 32% HP"
        );
    }

    #[test]
    fn test_attack_when_enemies_present() {
        let mut engine = make_engine();
        let state = combat_state(90, 80, 3);

        let mut found_attack = false;
        for _ in 0..50 {
            // Reset attack cooldown each iteration
            engine.last_attack = Instant::now() - Duration::from_secs(5);
            let d = engine.decide(&state);
            if matches!(d.action, Action::CastSkill { .. }) {
                found_attack = true;
                assert!(
                    d.delay < Duration::from_millis(500),
                    "attack delay too high: {:?}",
                    d.delay
                );
                break;
            }
        }
        assert!(found_attack, "should attack when enemies present and HP healthy");
    }

    #[test]
    fn test_no_attack_in_town() {
        let mut engine = make_engine();
        let mut state = FrameState::default();
        state.in_town = true;
        state.enemy_count = 0;

        for _ in 0..20 {
            let d = engine.decide(&state);
            assert!(
                !matches!(d.action, Action::CastSkill { .. }),
                "should not attack in town"
            );
        }
    }

    #[test]
    fn test_kite_when_overwhelmed() {
        let mut engine = make_engine();
        let state = combat_state(85, 70, 10);

        let mut found_kite = false;
        for _ in 0..50 {
            engine.last_attack = Instant::now() - Duration::from_secs(5);
            let d = engine.decide(&state);
            if matches!(d.action, Action::MoveTo { .. }) && d.reason.contains("kite") {
                found_kite = true;
                break;
            }
        }
        assert!(found_kite, "should kite when enemy count > threshold");
    }

    #[test]
    fn test_survival_delay_capped() {
        let mut engine = make_engine();
        for _ in 0..100 {
            let delay = engine.survival_delay();
            assert!(
                delay.as_millis() <= engine.config.humanization.survival_max_delay_ms as u128,
                "survival delay {} exceeded cap {}",
                delay.as_millis(),
                engine.config.humanization.survival_max_delay_ms
            );
        }
    }

    #[test]
    fn test_normal_delay_distribution() {
        let mut engine = make_engine();
        let mut delays = Vec::with_capacity(1000);
        for _ in 0..1000 {
            delays.push(engine.normal_delay().as_millis() as f64);
        }
        let mean: f64 = delays.iter().sum::<f64>() / delays.len() as f64;
        let variance: f64 = delays.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / delays.len() as f64;
        let stddev = variance.sqrt();
        assert!(mean > 150.0 && mean < 500.0, "unexpected mean: {:.1}", mean);
        assert!(stddev > 30.0, "variance too low: {:.1}", stddev);
    }

    #[test]
    fn test_loot_priority_runes_first() {
        use crate::vision::LootLabel;
        let engine = make_engine();

        let mut state = FrameState::default();
        state.in_town = false;
        state.enemy_count = 0;
        state.in_combat = false;
        state.loot_label_count = 3;
        state.loot_labels[0] = LootLabel { x: 100, y: 100, quality: ItemQuality::Rare, text_hash: 1 };
        state.loot_labels[1] = LootLabel { x: 401, y: 301, quality: ItemQuality::Magic, text_hash: 2 };
        state.loot_labels[2] = LootLabel { x: 600, y: 500, quality: ItemQuality::Rune, text_hash: 3 };

        let d = engine.check_loot(&state).expect("should want to loot");
        match d.action {
            Action::PickupLoot { screen_x, screen_y } => {
                assert_eq!(screen_x, 600, "should target rune");
                assert_eq!(screen_y, 500, "should target rune");
            }
            _ => panic!("expected PickupLoot"),
        }
    }

    #[test]
    fn test_attack_slots_boss_target() {
        let mut config = AgentConfig::default();
        config.combat.attack_slots.boss_primary = Some('f');
        config.combat.attack_slots.boss_untimed = Some('g');
        config.combat.attack_slots.mob_primary = Some('h');
        let mut engine = DecisionEngine::new(config);
        // Disable humanization miss rate for deterministic test
        engine.config.humanization.skill_miss_rate = 0.0;

        let mut state = combat_state(90, 80, 3);
        state.boss_present = true;
        state.nearest_enemy_hp_pct = 100;

        let mut found_boss_skill = false;
        for _ in 0..50 {
            engine.last_attack = Instant::now() - Duration::from_secs(5);
            engine.last_preattack = Instant::now(); // prevent preattack from firing
            let d = engine.decide(&state);
            if let Action::CastSkill { key, .. } = d.action {
                // Should use boss_primary ('f') or boss_untimed ('g'), never mob_primary ('h')
                assert!(key == 'f' || key == 'g',
                    "boss target should use boss slots, got: {}", key);
                found_boss_skill = true;
                break;
            }
        }
        assert!(found_boss_skill, "should use boss attack slot");
    }

    #[test]
    fn test_attack_slots_immune_fallback() {
        let mut config = AgentConfig::default();
        config.combat.attack_slots.immune_primary = Some('h');
        config.combat.attack_slots.mob_primary = Some('f');
        config.combat.immunity_fallback_key = Some('h');
        let mut engine = DecisionEngine::new(config);
        engine.config.humanization.skill_miss_rate = 0.0;

        let mut state = combat_state(90, 80, 2);
        state.immune_detected = true;

        let mut found_immune_skill = false;
        for _ in 0..50 {
            engine.last_attack = Instant::now() - Duration::from_secs(5);
            engine.last_preattack = Instant::now();
            let d = engine.decide(&state);
            if let Action::CastSkill { key, .. } = d.action {
                assert_eq!(key, 'h', "immune target should use immune slot");
                found_immune_skill = true;
                break;
            }
        }
        assert!(found_immune_skill, "should use immune attack slot");
    }

    #[test]
    fn test_mana_chicken() {
        let mut config = AgentConfig::default();
        config.survival.mana_chicken_pct = 10;
        let mut engine = DecisionEngine::new(config);

        let state = combat_state(80, 5, 3);

        let d = engine.decide(&state);
        assert!(
            matches!(d.action, Action::ChickenQuit),
            "should chicken when mana is below mana_chicken_pct, got: {:?}",
            d.action
        );
    }

    #[test]
    fn test_mana_chicken_disabled() {
        let mut config = AgentConfig::default();
        config.survival.mana_chicken_pct = 0; // disabled
        let mut engine = DecisionEngine::new(config);

        let state = combat_state(80, 5, 3);

        let d = engine.decide(&state);
        assert!(
            !matches!(d.action, Action::ChickenQuit),
            "should NOT chicken when mana_chicken_pct is 0"
        );
    }

    #[test]
    fn test_static_field_on_boss() {
        let mut config = AgentConfig::default();
        config.combat.static_field = Some(StaticFieldConfig {
            hotkey: 'e',
            until_hp_pct: 40,
            max_casts: 5,
        });
        let mut engine = DecisionEngine::new(config);
        engine.config.humanization.skill_miss_rate = 0.0;
        engine.last_static_field = Instant::now() - Duration::from_secs(5);

        let mut state = combat_state(90, 80, 1);
        state.boss_present = true;
        state.nearest_enemy_hp_pct = 80; // above 40% threshold

        let mut found_static = false;
        for _ in 0..50 {
            engine.last_attack = Instant::now() - Duration::from_secs(5);
            engine.last_static_field = Instant::now() - Duration::from_secs(5);
            engine.last_preattack = Instant::now();
            let d = engine.decide(&state);
            if let Action::CastSkill { key, .. } = d.action {
                if key == 'e' {
                    found_static = true;
                    break;
                }
            }
        }
        assert!(found_static, "should cast static field on boss above threshold");
    }

    #[test]
    fn test_mf_switch_on_low_boss() {
        let mut config = AgentConfig::default();
        config.combat.mf_switch_pct = 15;
        let mut engine = DecisionEngine::new(config);

        let mut state = combat_state(90, 80, 1);
        state.boss_present = true;
        state.nearest_enemy_hp_pct = 10; // below 15% threshold

        let mut found_switch = false;
        for _ in 0..50 {
            engine.last_attack = Instant::now() - Duration::from_secs(5);
            engine.last_preattack = Instant::now();
            engine.on_weapon_switch = false;
            let d = engine.decide(&state);
            if matches!(d.action, Action::SwitchWeapon) {
                found_switch = true;
                break;
            }
        }
        assert!(found_switch, "should switch to MF weapon when boss HP below threshold");
    }

    #[test]
    fn test_dodge_at_low_hp() {
        let mut config = AgentConfig::default();
        config.combat.dodge = true;
        config.combat.dodge_hp = 60;
        config.combat.dodge_range = 15;
        config.combat.kite_threshold = 4;
        let mut engine = DecisionEngine::new(config);

        let mut state = combat_state(45, 80, 6); // HP below 60, enemies above kite threshold
        state.nearest_enemy_x = 380;
        state.nearest_enemy_y = 280;

        let mut found_dodge = false;
        for _ in 0..50 {
            let d = engine.decide(&state);
            if matches!(d.action, Action::Dodge { .. }) {
                found_dodge = true;
                break;
            }
        }
        assert!(found_dodge, "should dodge when HP low and enemies close");
    }

    #[test]
    fn test_target_type_derivation() {
        let engine = make_engine();

        let mut state = FrameState::default();
        state.in_town = false;
        state.in_combat = true;

        // Normal mob
        state.boss_present = false;
        state.champion_present = false;
        state.immune_detected = false;
        assert_eq!(engine.derive_target_type(&state), TargetType::Normal);

        // Boss
        state.boss_present = true;
        assert_eq!(engine.derive_target_type(&state), TargetType::Boss);

        // Immune takes priority over boss
        state.immune_detected = true;
        assert_eq!(engine.derive_target_type(&state), TargetType::Immune);

        // Champion
        state.boss_present = false;
        state.immune_detected = false;
        state.champion_present = true;
        assert_eq!(engine.derive_target_type(&state), TargetType::Champion);
    }
}
