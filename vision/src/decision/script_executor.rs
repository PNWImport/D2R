//! Script step executor — translates ScriptStep plans into frame-by-frame Actions.
//!
//! This is the missing bridge between progression.rs (what to do) and the
//! input system (how to do it). Each ScriptStep variant maps to a sequence
//! of screen interactions driven by FrameState visual feedback.
//!
//! Design principles:
//!   - One step active at a time, driven by repeated `tick()` calls at 25 Hz
//!   - Each tick returns an Action + delay (same as DecisionEngine)
//!   - Steps complete when a visual condition is met OR a timeout fires
//!   - Survival (potions, chicken) is handled by DecisionEngine, which runs
//!     BEFORE the executor each frame — executor only controls navigation/interaction

use super::engine::{Action, Decision, DecisionEngine};
use super::progression::{ScriptStep, VisualCue};
use crate::vision::FrameState;
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════
// WAYPOINT COORDINATES — screen positions for WP area list entries
// Base coordinates at 800×600, scaled to actual resolution via sx()/sy().
// Waypoint panel is ~300px wide (at base), centered, with area
// names listed vertically. Each act has a tab at the top.
// ═══════════════════════════════════════════════════════════════

/// Scale a base-800×600 X coordinate to the actual frame width.
#[inline(always)]
fn sx(v: i32, w: u16) -> i32 {
    (v as f32 * w as f32 / 800.0) as i32
}

/// Scale a base-800×600 Y coordinate to the actual frame height.
#[inline(always)]
fn sy(v: i32, h: u16) -> i32 {
    (v as f32 * h as f32 / 600.0) as i32
}

/// Waypoint panel act tab positions (top of WP panel)
const WP_ACT_TABS: [(i32, i32); 5] = [
    (263, 80), // Act 1
    (323, 80), // Act 2
    (383, 80), // Act 3
    (443, 80), // Act 4
    (503, 80), // Act 5
];

/// Waypoint area list — Y positions for each entry within an act tab.
/// The WP list starts at ~Y=110 and each entry is ~20px apart.
/// X is constant at ~400 (center of panel).
const WP_LIST_X: i32 = 400;
const WP_LIST_Y_START: i32 = 115;
const WP_LIST_Y_STEP: i32 = 21;

/// Map a destination area name to (act_tab_index, entry_index) in the WP panel.
/// Returns None if the area doesn't have a waypoint.
fn wp_panel_location(destination: &str) -> Option<(usize, usize)> {
    // Act 1 waypoints (in WP menu order)
    const ACT1: &[&str] = &[
        "Rogue Encampment",
        "Cold Plains",
        "Stony Field",
        "Dark Wood",
        "Black Marsh",
        "Outer Cloister",
        "Jail Level 1",
        "Inner Cloister",
        "Catacombs Level 2",
    ];
    // Act 2 waypoints
    const ACT2: &[&str] = &[
        "Lut Gholein",
        "Sewers Level 2",
        "Dry Hills",
        "Halls of the Dead Level 2",
        "Far Oasis",
        "Lost City",
        "Palace Cellar Level 1",
        "Arcane Sanctuary",
        "Canyon of the Magi",
    ];
    // Act 3 waypoints
    const ACT3: &[&str] = &[
        "Kurast Docks",
        "Spider Forest",
        "Great Marsh",
        "Flayer Jungle",
        "Lower Kurast",
        "Kurast Bazaar",
        "Upper Kurast",
        "Travincal",
        "Durance of Hate Level 2",
    ];
    // Act 4 waypoints
    const ACT4: &[&str] = &[
        "The Pandemonium Fortress",
        "City of the Damned",
        "River of Flame",
    ];
    // Act 5 waypoints
    const ACT5: &[&str] = &[
        "Harrogath",
        "Frigid Highlands",
        "Arreat Plateau",
        "Crystalline Passage",
        "Frozen Tundra",
        "Glacial Trail",
        "Halls of Pain",
        "The Ancients' Way",
        "Worldstone Keep Level 2",
    ];

    let acts: &[&[&str]] = &[ACT1, ACT2, ACT3, ACT4, ACT5];
    for (act_idx, entries) in acts.iter().enumerate() {
        for (entry_idx, &name) in entries.iter().enumerate() {
            if name == destination {
                return Some((act_idx, entry_idx));
            }
        }
    }
    None
}

/// Screen coordinates for a WP entry given act tab + entry index.
fn wp_entry_coords(_act_idx: usize, entry_idx: usize) -> (i32, i32) {
    (
        WP_LIST_X,
        WP_LIST_Y_START + entry_idx as i32 * WP_LIST_Y_STEP,
    )
}

/// Where the waypoint object sits in each act's town (screen coords at 800x600).
/// Used by execute_waypoint to walk to the WP before trying to open it.
fn town_wp_object_position(act: u8) -> (i32, i32) {
    match act {
        1 => (120, 280), // Rogue Encampment — WP near camp exit
        2 => (264, 288), // Lut Gholein — WP south of town
        3 => (229, 306), // Kurast Docks — WP south of docks
        4 => (158, 277), // Pandemonium Fortress — WP inside fortress
        _ => (210, 172), // Harrogath — WP near Malah
    }
}

/// Returns true if this NPC requires clicking a travel dialog option
/// rather than pressing Esc (e.g., Warriv to Act 2, Tyrael to Act 5).
fn is_travel_npc(npc: &str) -> bool {
    matches!(npc, "Warriv" | "Meshif" | "Jerhyn")
}

/// Screen position of the "Travel" button in a travel NPC's dialog.
/// These dialogs have a "Travel to <Act>" button near screen center.
fn travel_button_position() -> (i32, i32) {
    (400, 335) // Travel option sits in lower half of dialog box
}

// ═══════════════════════════════════════════════════════════════
// NPC NAME → SCREEN POSITION LOOKUP
// ═══════════════════════════════════════════════════════════════

/// Get approximate screen position for a named NPC in a given act.
/// These are 800×600 coordinates near where the NPC stands in town.
fn npc_position(npc: &str, act: u8) -> (i32, i32) {
    match (npc, act) {
        // Act 1
        ("Akara", 1) => (155, 72),
        ("Kashya", 1) => (466, 236),
        ("Charsi", 1) => (257, 209),
        ("Gheed", 1) => (323, 318),
        ("Warriv", 1) => (85, 233),
        ("Cain", 1) => (155, 155),
        // Act 2
        ("Atma", 2) => (330, 158),
        ("Drognan", 2) => (196, 93),
        ("Fara", 2) => (260, 142),
        ("Lysander", 2) => (192, 265),
        ("Greiz", 2) => (457, 218),
        ("Jerhyn", 2) => (380, 78),
        ("Meshif", 2) => (440, 310),
        ("Tyrael", 2) => (400, 300), // appears in Duriel's tomb
        ("Cain", 2) => (196, 93),
        // Act 3
        ("Ormus", 3) => (307, 170),
        ("Hratli", 3) => (226, 63),
        ("Asheara", 3) => (408, 95),
        ("Alkor", 3) => (185, 210),
        ("Cain", 3) => (307, 170),
        // Act 4
        ("Tyrael", 4) => (295, 190),
        ("Jamella", 4) => (152, 107),
        ("Halbu", 4) => (181, 155),
        ("Cain", 4) => (152, 107),
        // Act 5
        ("Malah", 5) => (328, 63),
        ("Larzuk", 5) => (135, 142),
        ("Qual-Kehk", 5) => (458, 147),
        ("Anya", 5) => (385, 154),
        ("Nihlathak", 5) => (490, 110),
        ("Cain", 5) => (385, 154),
        // Fallback: center of town
        _ => (400, 300),
    }
}

// ═══════════════════════════════════════════════════════════════
// SUB-STEP STATE — tracks progress within a single ScriptStep
// ═══════════════════════════════════════════════════════════════

/// Internal phase within a multi-tick ScriptStep.
/// Most steps require: approach → interact → confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum SubStep {
    /// Walking toward a target position.
    Approaching,
    /// Clicking/interacting with the target.
    Interacting,
    /// Waiting for a visual confirmation (area transition, dialog, etc.)
    WaitingForConfirm,
    /// Step is done — advance to next ScriptStep.
    Done,
}

// ═══════════════════════════════════════════════════════════════
// SCRIPT EXECUTOR
// ═══════════════════════════════════════════════════════════════

/// Executes ScriptStep plans by converting them into per-frame Actions.
///
/// Lifecycle:
///   1. GameManager loads a script plan via `load_plan(steps)`
///   2. Each frame, GameManager calls `tick(state)` to get a Decision
///   3. Executor returns actions for the current step
///   4. When a step completes, executor auto-advances
///   5. When all steps complete, `is_done()` returns true
pub struct ScriptExecutor {
    steps: Vec<ScriptStep>,
    index: usize,
    sub_step: SubStep,
    step_started: Instant,
    sub_step_started: Instant,
    rng: StdRng,

    /// Area name we last saw — for detecting transitions.
    last_area: [u8; 32],
    last_area_len: u8,

    /// How many ticks we've spent in the current sub-step (anti-stuck).
    sub_step_ticks: u32,

    /// Waypoint: which act tab to click, which entry to click.
    wp_act_idx: usize,
    wp_entry_idx: usize,
    /// WP flow: 0=open WP, 1=click act tab, 2=click entry, 3=wait load
    wp_phase: u8,

    /// For ClearArea: how many ticks with zero enemies before we call it cleared.
    clear_idle_ticks: u32,

    /// For KillTarget: how many ticks the boss has been dead / absent.
    kill_idle_ticks: u32,

    /// Movement direction seed for WalkToExit exploration.
    walk_angle: f32,
    /// How many walk attempts without seeing the target area.
    walk_attempts: u32,
}

impl Default for ScriptExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptExecutor {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            index: 0,
            sub_step: SubStep::Approaching,
            step_started: Instant::now(),
            sub_step_started: Instant::now(),
            rng: StdRng::from_entropy(),
            last_area: [0u8; 32],
            last_area_len: 0,
            sub_step_ticks: 0,
            wp_act_idx: 0,
            wp_entry_idx: 0,
            wp_phase: 0,
            clear_idle_ticks: 0,
            kill_idle_ticks: 0,
            walk_angle: 0.0,
            walk_attempts: 0,
        }
    }

    /// Load a new script plan. Resets all internal state.
    pub fn load_plan(&mut self, steps: Vec<ScriptStep>) {
        self.steps = steps;
        self.index = 0;
        self.reset_sub_step();
    }

    /// True when all steps have been executed (or plan is empty).
    pub fn is_done(&self) -> bool {
        self.index >= self.steps.len()
    }

    /// Current step index.
    pub fn step_index(&self) -> usize {
        self.index
    }

    /// Total steps in plan.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Get the current ScriptStep (if any).
    pub fn current_step(&self) -> Option<&ScriptStep> {
        self.steps.get(self.index)
    }

    /// Check if current step is a RequireLevel gate. Returns the min level
    /// if so, None otherwise.
    pub fn level_gate(&self) -> Option<u8> {
        match self.steps.get(self.index) {
            Some(ScriptStep::RequireLevel { min_level }) => Some(*min_level),
            _ => None,
        }
    }

    /// Advance past current step (used externally for TownChores which
    /// GameManager handles directly).
    pub fn skip_step(&mut self) {
        self.advance();
    }

    // ─── Main Tick ──────────────────────────────────────────────

    /// Called each frame (25 Hz). Returns the action for the current step,
    /// or None if the plan is done / needs external handling (TownChores).
    ///
    /// `engine` is borrowed for combat delegation during ClearArea/KillTarget.
    pub fn tick(&mut self, state: &FrameState, engine: &mut DecisionEngine) -> Option<Decision> {
        if self.is_done() {
            return None;
        }

        self.sub_step_ticks += 1;

        // Detect area changes
        if state.area_name_len > 0
            && (state.area_name_len != self.last_area_len
                || state.area_name[..state.area_name_len as usize]
                    != self.last_area[..self.last_area_len as usize])
        {
            self.last_area = state.area_name;
            self.last_area_len = state.area_name_len;
        }

        // Clone step to avoid borrow issues
        let step = self.steps[self.index].clone();
        match step {
            ScriptStep::TownChores => {
                // Signal to GameManager to handle town chores externally.
                // GameManager should call skip_step() when chores are done.
                None
            }
            ScriptStep::RequireLevel { min_level } => {
                // Handled externally by GameManager (check_level_gate).
                // If we reach here, the gate passed — advance.
                if state.char_level >= min_level {
                    self.advance();
                    // Re-tick for next step
                    self.tick(state, engine)
                } else {
                    None // Signal to GameManager: gate failed
                }
            }
            ScriptStep::RetryNextGame => {
                // Signal to GameManager to push retry.
                None
            }
            ScriptStep::UseWaypoint { destination } => self.execute_waypoint(state, destination),
            ScriptStep::WalkToExit { target_area } => self.execute_walk_to_exit(state, target_area),
            ScriptStep::ClearArea => self.execute_clear_area(state, engine),
            ScriptStep::KillTarget { name } => self.execute_kill_target(state, engine, name),
            ScriptStep::LootArea => self.execute_loot_area(state),
            ScriptStep::TownPortal => self.execute_town_portal(state),
            ScriptStep::TalkToNpc { npc, act } => self.execute_talk_to_npc(state, npc, act),
            ScriptStep::InteractObject { name } => self.execute_interact_object(state, name),
            ScriptStep::WaitForCue { cue, timeout_secs } => {
                self.execute_wait_for_cue(state, cue, timeout_secs)
            }
        }
    }

    // ─── Step Implementations ───────────────────────────────────

    /// UseWaypoint: walk to WP object → open panel → click act tab → click destination → wait for load.
    ///
    /// Phase 0: Walk toward the town waypoint object (1.2s)
    /// Phase 1: Open the WP panel (click WP object, retry until waypoint_menu_open)
    /// Phase 2: Click act tab (if needed — already on correct act, may be a no-op)
    /// Phase 3: Click destination entry
    /// Phase 4: Wait for loading screen + area transition
    fn execute_waypoint(&mut self, state: &FrameState, destination: &str) -> Option<Decision> {
        // Resolve WP panel coordinates on first entry (phase 0, tick 1)
        if self.wp_phase == 0 && self.sub_step_ticks == 1 {
            if let Some((act_idx, entry_idx)) = wp_panel_location(destination) {
                self.wp_act_idx = act_idx;
                self.wp_entry_idx = entry_idx;
            } else {
                tracing::warn!("No waypoint mapping for '{}' — skipping step", destination);
                self.advance();
                return Some(self.wait("wp: unknown destination, skipping"));
            }
        }

        match self.wp_phase {
            0 => {
                // Phase 0: Walk toward the town WP object so we're in range to click it.
                // Uses state.current_act to find the waypoint's position in this town.
                let (bwx, bwy) = town_wp_object_position(state.current_act);
                let (wx, wy) = (sx(bwx, state.frame_width), sy(bwy, state.frame_height));

                // If WP is already open, skip straight to tab selection
                if state.waypoint_menu_open {
                    self.wp_phase = 2;
                    self.sub_step_ticks = 0;
                    return self.execute_waypoint(state, destination);
                }

                if self.sub_step_ticks < 30 {
                    // ~1.2s of walking toward the WP position
                    return Some(Decision {
                        action: Action::MoveTo {
                            screen_x: wx + self.rng.gen_range(-8..8),
                            screen_y: wy + self.rng.gen_range(-8..8),
                        },
                        delay: Duration::from_millis(self.rng.gen_range(100..200)),
                        priority: 2,
                        reason: "wp: walking to waypoint object",
                    });
                }

                // Now in range — transition to open phase
                self.wp_phase = 1;
                self.sub_step_ticks = 0;
                self.execute_waypoint(state, destination)
            }
            1 => {
                // Phase 1: Click WP object to open the panel. Retry every second.
                if state.waypoint_menu_open {
                    self.wp_phase = 2;
                    self.sub_step_ticks = 0;
                    return self.execute_waypoint(state, destination);
                }

                // Timeout: 6s. If WP still doesn't open, go back to walking phase.
                if self.sub_step_ticks > 150 {
                    tracing::warn!("WP open timeout — re-approaching WP object");
                    self.wp_phase = 0;
                    self.sub_step_ticks = 0;
                    return self.execute_waypoint(state, destination);
                }

                let (bwx, bwy) = town_wp_object_position(state.current_act);
                let (wx, wy) = (sx(bwx, state.frame_width), sy(bwy, state.frame_height));
                if self.sub_step_ticks % 20 == 1 {
                    // Click the WP every ~0.8s
                    Some(self.jittered_click(wx, wy, "wp: clicking waypoint object"))
                } else {
                    Some(self.wait("wp: waiting for panel to open"))
                }
            }
            2 => {
                // Phase 2: Click the correct act tab.
                // If the WP panel is already showing our act, this is just cosmetic.
                if self.sub_step_ticks < 5 {
                    return Some(self.wait("wp: settling before tab click"));
                }
                let (btx, bty) = WP_ACT_TABS[self.wp_act_idx];
                let (tx, ty) = (sx(btx, state.frame_width), sy(bty, state.frame_height));
                self.wp_phase = 3;
                self.sub_step_ticks = 0;
                Some(self.jittered_click(tx, ty, "wp: clicking act tab"))
            }
            3 => {
                // Phase 3: Click the destination entry
                if self.sub_step_ticks < 5 {
                    return Some(self.wait("wp: settling before entry click"));
                }
                let (bex, bey) = wp_entry_coords(self.wp_act_idx, self.wp_entry_idx);
                let (ex, ey) = (sx(bex, state.frame_width), sy(bey, state.frame_height));
                self.wp_phase = 4;
                self.sub_step_ticks = 0;
                Some(self.jittered_click(ex, ey, "wp: clicking destination entry"))
            }
            _ => {
                // Phase 4: Wait for loading screen → area transition
                if state.loading_screen {
                    return Some(self.wait("wp: on loading screen"));
                }
                // Loading screen ended and enough time has passed — arrived
                if !state.loading_screen && self.sub_step_ticks > 10 {
                    self.wp_phase = 0;
                    self.advance();
                    return Some(self.wait("wp: arrived at destination"));
                }
                // Hard timeout: 15s
                if self.sub_step_ticks > 375 {
                    tracing::warn!("WP travel timeout for '{}' — advancing", destination);
                    self.wp_phase = 0;
                    self.advance();
                }
                Some(self.wait("wp: waiting for area transition"))
            }
        }
    }

    /// WalkToExit: move toward screen edges until area name changes to target.
    /// Uses a semi-random exploration pattern (like kolbot's Pather.moveTo
    /// but vision-only — no map hack).
    fn execute_walk_to_exit(&mut self, state: &FrameState, target_area: &str) -> Option<Decision> {
        let current_area = state.area_name_str();

        // Check if we've arrived
        if current_area == target_area
            || (target_area.contains("Tal Rasha's") && current_area.starts_with("Tal Rasha's"))
        {
            tracing::info!("Arrived at {}", target_area);
            self.walk_attempts = 0;
            self.advance();
            return Some(self.wait("walk: arrived at target area"));
        }

        // If loading screen, wait
        if state.loading_screen {
            return Some(self.wait("walk: loading screen"));
        }

        self.walk_attempts += 1;

        // Anti-stuck: rotate exploration angle every ~80 ticks (3.2s)
        if self.walk_attempts % 80 == 0 {
            self.walk_angle += std::f32::consts::FRAC_PI_3; // rotate 60°
            if self.walk_angle > std::f32::consts::TAU {
                self.walk_angle -= std::f32::consts::TAU;
            }
        }

        // Jitter the angle slightly each tick for natural movement
        let jitter = self.rng.gen_range(-0.25f32..0.25);
        let angle = self.walk_angle + jitter;
        let distance = self.rng.gen_range(180.0f32..280.0);

        let tx = (state.char_screen_x as f32 + angle.cos() * distance) as i32;
        let ty = (state.char_screen_y as f32 + angle.sin() * distance) as i32;

        // Clamp to screen bounds (scaled to actual resolution)
        let tx = tx.clamp(sx(50, state.frame_width), sx(750, state.frame_width));
        let ty = ty.clamp(sy(50, state.frame_height), sy(550, state.frame_height));

        // If we're in combat, let engine handle it before walking
        if state.in_combat && state.enemy_count > 0 {
            return Some(self.wait("walk: in combat, waiting for clear"));
        }

        // Pick up any visible loot while walking
        if state.loot_label_count > 0 && !state.in_combat {
            let label = &state.loot_labels[0];
            return Some(Decision {
                action: Action::PickupLoot {
                    screen_x: label.x as i32,
                    screen_y: label.y as i32,
                },
                delay: Duration::from_millis(self.rng.gen_range(100..250)),
                priority: 5,
                reason: "walk: picking up loot en route",
            });
        }

        // Timeout: if we've been walking for 60s, step probably failed
        if self.walk_attempts > 1500 {
            tracing::warn!(
                "WalkToExit timeout after {}s for '{}' — advancing",
                self.walk_attempts / 25,
                target_area
            );
            self.walk_attempts = 0;
            self.advance();
            return Some(self.wait("walk: timeout, advancing"));
        }

        // Move toward exploration point (right-click for run/teleport)
        Some(Decision {
            action: Action::MoveTo {
                screen_x: tx,
                screen_y: ty,
            },
            delay: Duration::from_millis(self.rng.gen_range(80..200)),
            priority: 5,
            reason: "walk: exploring toward exit",
        })
    }

    /// ClearArea: let DecisionEngine handle combat. Step completes when
    /// no enemies are visible for ~3 seconds.
    fn execute_clear_area(
        &mut self,
        state: &FrameState,
        engine: &mut DecisionEngine,
    ) -> Option<Decision> {
        if state.in_combat && state.enemy_count > 0 {
            self.clear_idle_ticks = 0;
            // Delegate to combat engine
            return Some(engine.decide(state));
        }

        // No enemies visible
        self.clear_idle_ticks += 1;

        // Pick up loot between fights
        if state.loot_label_count > 0 {
            self.clear_idle_ticks = 0;
            let label = &state.loot_labels[0];
            return Some(Decision {
                action: Action::PickupLoot {
                    screen_x: label.x as i32,
                    screen_y: label.y as i32,
                },
                delay: Duration::from_millis(self.rng.gen_range(80..200)),
                priority: 5,
                reason: "clear: looting between fights",
            });
        }

        // After 3 seconds of no enemies, area is considered cleared
        if self.clear_idle_ticks > 75 {
            // 3s at 25Hz
            tracing::info!("Area cleared — {} seconds idle", self.clear_idle_ticks / 25);
            self.clear_idle_ticks = 0;
            self.advance();
            return Some(self.wait("clear: area cleared"));
        }

        // Move around to find more enemies
        let angle = self.rng.gen_range(0.0f32..std::f32::consts::TAU);
        let dist = self.rng.gen_range(120.0f32..240.0);
        let tx = (state.char_screen_x as f32 + angle.cos() * dist) as i32;
        let ty = (state.char_screen_y as f32 + angle.sin() * dist) as i32;

        Some(Decision {
            action: Action::MoveTo {
                screen_x: tx.clamp(sx(50, state.frame_width), sx(750, state.frame_width)),
                screen_y: ty.clamp(sy(50, state.frame_height), sy(550, state.frame_height)),
            },
            delay: Duration::from_millis(self.rng.gen_range(150..350)),
            priority: 6,
            reason: "clear: searching for enemies",
        })
    }

    /// KillTarget: delegate combat to engine, complete when boss is dead
    /// (no boss_present/champion_present for ~2 seconds after engagement).
    fn execute_kill_target(
        &mut self,
        state: &FrameState,
        engine: &mut DecisionEngine,
        _name: &str,
    ) -> Option<Decision> {
        // If boss/champion is present, engage
        if state.boss_present
            || state.champion_present
            || (state.in_combat && state.enemy_count > 0)
        {
            self.kill_idle_ticks = 0;
            return Some(engine.decide(state));
        }

        self.kill_idle_ticks += 1;

        // Pick up loot after kill
        if state.loot_label_count > 0 {
            let label = &state.loot_labels[0];
            return Some(Decision {
                action: Action::PickupLoot {
                    screen_x: label.x as i32,
                    screen_y: label.y as i32,
                },
                delay: Duration::from_millis(self.rng.gen_range(80..180)),
                priority: 5,
                reason: "kill: looting after target kill",
            });
        }

        // After 2 seconds with no boss visible, target is dead
        if self.kill_idle_ticks > 50 {
            tracing::info!("Target killed (or absent)");
            self.kill_idle_ticks = 0;
            self.advance();
            return Some(self.wait("kill: target down"));
        }

        // Search for the target — move around
        let angle = self.rng.gen_range(0.0f32..std::f32::consts::TAU);
        let dist = self.rng.gen_range(100.0f32..200.0);
        let tx = (state.char_screen_x as f32 + angle.cos() * dist) as i32;
        let ty = (state.char_screen_y as f32 + angle.sin() * dist) as i32;

        Some(Decision {
            action: Action::MoveTo {
                screen_x: tx.clamp(sx(50, state.frame_width), sx(750, state.frame_width)),
                screen_y: ty.clamp(sy(50, state.frame_height), sy(550, state.frame_height)),
            },
            delay: Duration::from_millis(self.rng.gen_range(100..300)),
            priority: 6,
            reason: "kill: searching for target",
        })
    }

    /// LootArea: pick up all visible loot labels, then advance.
    fn execute_loot_area(&mut self, state: &FrameState) -> Option<Decision> {
        if state.loot_label_count > 0 {
            // Pick the highest priority item (same logic as DecisionEngine)
            use crate::vision::ItemQuality;
            let labels = &state.loot_labels[..state.loot_label_count as usize];
            let best = labels.iter().min_by_key(|l| {
                let priority: i32 = match l.quality {
                    ItemQuality::Rune | ItemQuality::Unique => -10000,
                    ItemQuality::Set => -5000,
                    ItemQuality::Rare => -1000,
                    _ => 0,
                };
                let dx = l.x as i32 - state.char_screen_x as i32;
                let dy = l.y as i32 - state.char_screen_y as i32;
                priority + dx * dx + dy * dy
            });

            if let Some(label) = best {
                self.sub_step_ticks = 0;
                return Some(Decision {
                    action: Action::PickupLoot {
                        screen_x: label.x as i32,
                        screen_y: label.y as i32,
                    },
                    delay: Duration::from_millis(self.rng.gen_range(80..200)),
                    priority: 5,
                    reason: "loot: picking up item",
                });
            }
        }

        // No more loot visible — wait a moment then advance
        self.sub_step_ticks += 1;
        if self.sub_step_ticks > 25 {
            // 1s grace period
            self.advance();
            return Some(self.wait("loot: area looted"));
        }

        Some(self.wait("loot: checking for more items"))
    }

    /// TownPortal: cast TP, wait for town arrival.
    fn execute_town_portal(&mut self, state: &FrameState) -> Option<Decision> {
        match self.sub_step {
            SubStep::Approaching => {
                // Cast TP
                self.sub_step = SubStep::Interacting;
                self.sub_step_started = Instant::now();
                self.sub_step_ticks = 0;
                Some(Decision {
                    action: Action::TownPortal,
                    delay: Duration::from_millis(self.rng.gen_range(50..150)),
                    priority: 3,
                    reason: "tp: casting town portal",
                })
            }
            SubStep::Interacting => {
                // Wait for TP to appear, then click it
                if self.sub_step_ticks > 25 {
                    // 1s delay for cast animation
                    // Click slightly above character (TP appears nearby)
                    self.sub_step = SubStep::WaitingForConfirm;
                    self.sub_step_ticks = 0;
                    return Some(self.jittered_click(
                        state.char_screen_x as i32,
                        state.char_screen_y as i32 - 60,
                        "tp: clicking portal",
                    ));
                }
                Some(self.wait("tp: waiting for portal to appear"))
            }
            SubStep::WaitingForConfirm => {
                // Wait for town arrival
                if state.in_town {
                    self.advance();
                    return Some(self.wait("tp: arrived in town"));
                }
                if state.loading_screen {
                    return Some(self.wait("tp: loading"));
                }
                // Timeout: 10s
                if self.sub_step_ticks > 250 {
                    tracing::warn!("TP timeout — advancing anyway");
                    self.advance();
                    return Some(self.wait("tp: timeout"));
                }
                Some(self.wait("tp: waiting for town"))
            }
            SubStep::Done => {
                self.advance();
                Some(self.wait("tp: done"))
            }
        }
    }

    /// TalkToNpc: walk to NPC position → click to interact → wait for dialog.
    ///
    /// For "travel" NPCs (Warriv, Meshif, Jerhyn) the dialog has a travel button
    /// that must be clicked — pressing Esc just dismisses without traveling.
    /// For regular NPCs (quest reward, Cain ID, etc.) Esc is fine.
    fn execute_talk_to_npc(&mut self, state: &FrameState, npc: &str, act: u8) -> Option<Decision> {
        let (bx, by) = npc_position(npc, act);
        let npc_pos = (sx(bx, state.frame_width), sy(by, state.frame_height));
        let travel = is_travel_npc(npc);

        match self.sub_step {
            SubStep::Approaching => {
                // Walk toward NPC for ~1.5s
                if self.sub_step_ticks > 38 {
                    self.sub_step = SubStep::Interacting;
                    self.sub_step_ticks = 0;
                }
                Some(Decision {
                    action: Action::MoveTo {
                        screen_x: npc_pos.0 + self.rng.gen_range(-10..10),
                        screen_y: npc_pos.1 + self.rng.gen_range(-10..10),
                    },
                    delay: Duration::from_millis(self.rng.gen_range(150..350)),
                    priority: 3,
                    reason: "npc: walking to NPC",
                })
            }
            SubStep::Interacting => {
                // Click NPC to open dialog
                if self.sub_step_ticks > 15 {
                    self.sub_step = SubStep::WaitingForConfirm;
                    self.sub_step_ticks = 0;
                }
                Some(self.jittered_click(npc_pos.0, npc_pos.1, "npc: clicking NPC"))
            }
            SubStep::WaitingForConfirm => {
                if state.npc_dialog_open {
                    self.advance();
                    if travel {
                        // Travel NPCs: click the "Travel to <next act>" button.
                        // The travel button sits in the lower half of the dialog box.
                        let (tbx, tby) = travel_button_position();
                        let (bx, by) = (sx(tbx, state.frame_width), sy(tby, state.frame_height));
                        return Some(Decision {
                            action: Action::Click {
                                screen_x: bx + self.rng.gen_range(-10..10),
                                screen_y: by + self.rng.gen_range(-5..5),
                            },
                            delay: Duration::from_millis(self.rng.gen_range(400..700)),
                            priority: 3,
                            reason: "npc: clicking travel button",
                        });
                    } else {
                        // Regular NPC: quest reward, Cain ID, etc. — just close dialog.
                        return Some(Decision {
                            action: Action::CastSkill {
                                key: '\x1b',
                                screen_x: (state.frame_width / 2) as i32,
                                screen_y: (state.frame_height / 2) as i32,
                            },
                            delay: Duration::from_millis(self.rng.gen_range(300..600)),
                            priority: 3,
                            reason: "npc: closing dialog",
                        });
                    }
                }
                // Timeout: 5s
                if self.sub_step_ticks > 125 {
                    tracing::warn!("NPC dialog timeout for '{}' — advancing", npc);
                    self.advance();
                    return Some(self.wait("npc: timeout"));
                }
                // No dialog yet — keep trying to click the NPC every ~1s
                if self.sub_step_ticks % 25 == 12 {
                    return Some(self.jittered_click(npc_pos.0, npc_pos.1, "npc: retry click"));
                }
                Some(self.wait("npc: waiting for dialog"))
            }
            SubStep::Done => {
                self.advance();
                Some(self.wait("npc: done"))
            }
        }
    }

    /// InteractObject: click the named object on screen.
    /// For objects like waypoints, chests, altars — we click near the expected
    /// position and wait for a response (UI opens, loot drops, etc.)
    fn execute_interact_object(&mut self, state: &FrameState, name: &str) -> Option<Decision> {
        match self.sub_step {
            SubStep::Approaching => {
                // For most objects, click near center of screen where character
                // should be standing close to the object after navigation steps.
                // Specific objects have known offsets.
                let target = object_click_position(name, state);

                if self.sub_step_ticks > 30 {
                    self.sub_step = SubStep::Interacting;
                    self.sub_step_ticks = 0;
                }

                Some(Decision {
                    action: Action::MoveTo {
                        screen_x: target.0 + self.rng.gen_range(-15..15),
                        screen_y: target.1 + self.rng.gen_range(-10..10),
                    },
                    delay: Duration::from_millis(self.rng.gen_range(150..350)),
                    priority: 3,
                    reason: "object: approaching",
                })
            }
            SubStep::Interacting => {
                let target = object_click_position(name, state);
                if self.sub_step_ticks > 8 {
                    self.sub_step = SubStep::WaitingForConfirm;
                    self.sub_step_ticks = 0;
                }
                Some(self.jittered_click(target.0, target.1, "object: clicking"))
            }
            SubStep::WaitingForConfirm => {
                // Different objects have different confirmations
                let confirmed = match name {
                    "waypoint" => state.waypoint_menu_open,
                    "Horadric Cube" | "Horadric Cube Chest" => state.cube_open || state.stash_open,
                    _ => {
                        // Generic: wait 1.5s then assume success
                        self.sub_step_ticks > 38
                    }
                };

                if confirmed {
                    // Close any UI that opened (waypoint gets handled by UseWaypoint step)
                    self.advance();
                    return Some(self.wait("object: interaction complete"));
                }

                // Timeout: 8s
                if self.sub_step_ticks > 200 {
                    tracing::warn!("Object interaction timeout for '{}' — advancing", name);
                    self.advance();
                    return Some(self.wait("object: timeout"));
                }

                Some(self.wait("object: waiting for response"))
            }
            SubStep::Done => {
                self.advance();
                Some(self.wait("object: done"))
            }
        }
    }

    /// WaitForCue: block until a visual cue appears or timeout.
    fn execute_wait_for_cue(
        &mut self,
        state: &FrameState,
        cue: VisualCue,
        timeout_secs: u8,
    ) -> Option<Decision> {
        let cue_detected = match cue {
            VisualCue::QuestCompleteBanner => state.quest_complete_banner,
            VisualCue::AreaTransition => {
                // Area changed since step started
                let current = state.area_name_str();
                !current.is_empty()
                    && (self.last_area_len == 0
                        || current.as_bytes() != &self.last_area[..self.last_area_len as usize])
            }
            VisualCue::NpcDialogOpen => state.npc_dialog_open,
            VisualCue::WaypointMenuOpen => state.waypoint_menu_open,
            VisualCue::LoadingScreenEnd => !state.loading_screen && self.sub_step_ticks > 10,
        };

        if cue_detected {
            self.advance();
            return Some(self.wait("cue: detected"));
        }

        let timeout_ticks = timeout_secs as u32 * 25;
        if self.sub_step_ticks > timeout_ticks {
            tracing::warn!(
                "WaitForCue timeout ({:?}, {}s) — advancing",
                cue,
                timeout_secs
            );
            self.advance();
            return Some(self.wait("cue: timeout"));
        }

        Some(self.wait("cue: waiting"))
    }

    // ─── Internal Helpers ───────────────────────────────────────

    fn advance(&mut self) {
        self.index += 1;
        self.reset_sub_step();
    }

    fn reset_sub_step(&mut self) {
        self.sub_step = SubStep::Approaching;
        self.sub_step_started = Instant::now();
        self.step_started = Instant::now();
        self.sub_step_ticks = 0;
        self.wp_phase = 0;
        self.clear_idle_ticks = 0;
        self.kill_idle_ticks = 0;
        self.walk_attempts = 0;
        self.walk_angle = self.rng.gen_range(0.0..std::f32::consts::TAU);
    }

    fn wait(&self, reason: &'static str) -> Decision {
        Decision {
            action: Action::Wait,
            delay: Duration::from_millis(40),
            priority: 5,
            reason,
        }
    }

    fn jittered_click(&mut self, x: i32, y: i32, reason: &'static str) -> Decision {
        Decision {
            action: Action::Click {
                screen_x: x + self.rng.gen_range(-6..=6),
                screen_y: y + self.rng.gen_range(-4..=4),
            },
            delay: Duration::from_millis(self.rng.gen_range(120..350)),
            priority: 3,
            reason,
        }
    }
}

/// Get the screen position to click for a named game object.
/// Falls back to character center if unknown.
fn object_click_position(name: &str, state: &FrameState) -> (i32, i32) {
    let cx = state.char_screen_x as i32;
    let cy = state.char_screen_y as i32;
    let w = state.frame_width;
    let h = state.frame_height;

    match name {
        // Waypoints are usually clicked slightly above character
        "waypoint" => (cx, cy - sy(30, h)),

        // Stash: to the right in most acts
        "Stash" => (cx + sx(60, w), cy - sy(20, h)),

        // Horadric Cube: open inventory first, cube is in the inventory grid
        "Horadric Cube" => (cx + sx(120, w), cy + sy(60, h)),

        // Cairn Stones in Stony Field: spread around character
        "Cairn Stone" => (cx + sx(40, w), cy - sy(50, h)),

        // Tree of Inifuss: slightly off-center
        "Tree of Inifuss" => (cx + sx(30, w), cy - sy(40, h)),

        // Cain's Gibbet (cage in Tristram)
        "Cain's Gibbet" => (cx, cy - sy(40, h)),

        // Chest objects: click near center
        "Horadric Cube Chest"
        | "Khalim's Eye Chest"
        | "Khalim's Heart Chest"
        | "Khalim's Brain Chest"
        | "Staff of Kings Chest"
        | "Super Chest" => (cx + sx(20, w), cy - sy(25, h)),

        // Quest objects
        "Horadric Malus" => (cx + sx(15, w), cy - sy(30, h)),
        "Tainted Sun Altar" => (cx, cy - sy(35, h)),
        "Lam Esen's Tome" => (cx + sx(10, w), cy - sy(30, h)),
        "Horadric Staff Orifice" => (cx, cy - sy(40, h)),
        "Compelling Orb" => (cx, cy - sy(40, h)),
        "Hellforge" => (cx + sx(20, w), cy - sy(30, h)),
        "Altar of the Heavens" => (cx, cy - sy(45, h)),

        // Portals
        "Red Portal" | "Anya Portal" => (cx, cy - sy(50, h)),

        // Seal levers in Chaos Sanctuary
        "Vizier Seal" | "Seis Seal" | "Infector Seal" => (cx + sx(25, w), cy - sy(35, h)),

        // Wirt's Body in Tristram
        "Wirt's Body" => (cx + sx(40, w), cy + sy(20, h)),

        // Prison doors in Frigid Highlands
        "Prison Door" => (cx + sx(30, w), cy - sy(20, h)),

        // Frozen Anya
        "Frozen Anya" => (cx, cy - sy(35, h)),

        // Journal in Arcane Sanctuary
        "Journal" => (cx + sx(10, w), cy - sy(30, h)),

        // Default: slightly above character (scaled to resolution)
        _ => (cx, cy - sy(30, h)),
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentConfig;
    use crate::vision::FrameState;

    fn make_executor() -> ScriptExecutor {
        ScriptExecutor::new()
    }

    fn make_engine() -> DecisionEngine {
        DecisionEngine::new(AgentConfig::default())
    }

    fn town_state() -> FrameState {
        let mut s = FrameState::default();
        s.in_town = true;
        s.at_menu = false;
        s.loading_screen = false;
        s.char_screen_x = 640;
        s.char_screen_y = 360;
        s
    }

    fn field_state(area: &str) -> FrameState {
        let mut s = FrameState::default();
        s.in_town = false;
        s.at_menu = false;
        s.loading_screen = false;
        s.char_screen_x = 640;
        s.char_screen_y = 360;
        s.hp_pct = 90;
        s.mana_pct = 80;
        s.set_area_name(area);
        s
    }

    #[test]
    fn test_empty_plan_is_done() {
        let mut exec = make_executor();
        exec.load_plan(vec![]);
        assert!(exec.is_done());
    }

    #[test]
    fn test_town_chores_returns_none() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::TownChores]);

        let state = town_state();
        let result = exec.tick(&state, &mut engine);
        assert!(
            result.is_none(),
            "TownChores should return None for external handling"
        );
    }

    #[test]
    fn test_require_level_passes() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![
            ScriptStep::RequireLevel { min_level: 5 },
            ScriptStep::TownChores,
        ]);

        let mut state = town_state();
        state.char_level = 10;

        let result = exec.tick(&state, &mut engine);
        // Should auto-advance past RequireLevel since level >= 5,
        // then hit TownChores which returns None
        assert!(result.is_none());
        assert_eq!(exec.step_index(), 1); // advanced to TownChores
    }

    #[test]
    fn test_require_level_fails() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::RequireLevel { min_level: 20 }]);

        let mut state = town_state();
        state.char_level = 5;

        let result = exec.tick(&state, &mut engine);
        assert!(result.is_none(), "Level gate failure should return None");
        assert_eq!(exec.step_index(), 0); // did not advance
    }

    #[test]
    fn test_town_portal_starts_cast() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::TownPortal]);

        let state = field_state("Blood Moor");
        let result = exec.tick(&state, &mut engine);
        assert!(result.is_some());
        let d = result.unwrap();
        assert!(
            matches!(d.action, Action::TownPortal),
            "First tick should cast TP"
        );
    }

    #[test]
    fn test_town_portal_completes_on_town() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::TownPortal]);

        // Simulate: cast → waiting → arrive in town
        let mut state = field_state("Blood Moor");
        let _ = exec.tick(&state, &mut engine); // cast

        // Tick through interacting phase
        for _ in 0..30 {
            let _ = exec.tick(&state, &mut engine);
        }

        // Now we're in town
        state.in_town = true;
        state.set_area_name("Rogue Encampment");
        let _ = exec.tick(&state, &mut engine);

        assert!(exec.is_done(), "Should complete after arriving in town");
    }

    #[test]
    fn test_clear_area_completes_after_idle() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::ClearArea]);

        let mut state = field_state("Den of Evil");
        state.enemy_count = 0;
        state.in_combat = false;

        // Tick through the idle detection period (75+ ticks = 3s)
        for _ in 0..80 {
            let _ = exec.tick(&state, &mut engine);
        }

        assert!(
            exec.is_done(),
            "ClearArea should complete after idle period"
        );
    }

    #[test]
    fn test_clear_area_delegates_combat() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::ClearArea]);

        let mut state = field_state("Den of Evil");
        state.enemy_count = 3;
        state.in_combat = true;
        state.nearest_enemy_x = 672;
        state.nearest_enemy_y = 300;
        state.nearest_enemy_hp_pct = 80;

        engine.last_attack = std::time::Instant::now() - Duration::from_secs(5);

        let result = exec.tick(&state, &mut engine);
        assert!(result.is_some());
        assert!(!exec.is_done(), "Should not complete while enemies present");
    }

    #[test]
    fn test_kill_target_completes_after_death() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::KillTarget { name: "Andariel" }]);

        let mut state = field_state("Catacombs Level 4");
        state.boss_present = false;
        state.enemy_count = 0;
        state.in_combat = false;

        // Tick through kill idle detection (50+ ticks = 2s)
        for _ in 0..55 {
            let _ = exec.tick(&state, &mut engine);
        }

        assert!(
            exec.is_done(),
            "KillTarget should complete when target absent"
        );
    }

    #[test]
    fn test_walk_to_exit_completes_on_arrival() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::WalkToExit {
            target_area: "Cold Plains",
        }]);

        // Start in Blood Moor
        let mut state = field_state("Blood Moor");
        let _ = exec.tick(&state, &mut engine);
        assert!(!exec.is_done());

        // Arrive at Cold Plains
        state.set_area_name("Cold Plains");
        let _ = exec.tick(&state, &mut engine);
        assert!(exec.is_done(), "Should complete when area matches target");
    }

    #[test]
    fn test_loot_area_with_no_loot() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::LootArea]);

        let state = field_state("Blood Moor");

        // Tick through the grace period (25 ticks)
        for _ in 0..30 {
            let _ = exec.tick(&state, &mut engine);
        }

        assert!(
            exec.is_done(),
            "LootArea should complete when no loot visible"
        );
    }

    #[test]
    fn test_wait_for_cue_detects_quest_banner() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::WaitForCue {
            cue: VisualCue::QuestCompleteBanner,
            timeout_secs: 10,
        }]);

        let mut state = field_state("Den of Evil");
        state.quest_complete_banner = false;
        let _ = exec.tick(&state, &mut engine);
        assert!(!exec.is_done());

        state.quest_complete_banner = true;
        let _ = exec.tick(&state, &mut engine);
        assert!(exec.is_done());
    }

    #[test]
    fn test_wait_for_cue_timeout() {
        let mut exec = make_executor();
        let mut engine = make_engine();
        exec.load_plan(vec![ScriptStep::WaitForCue {
            cue: VisualCue::QuestCompleteBanner,
            timeout_secs: 2,
        }]);

        let state = field_state("Den of Evil");

        // 2s = 50 ticks, tick past timeout
        for _ in 0..55 {
            let _ = exec.tick(&state, &mut engine);
        }

        assert!(exec.is_done(), "Should advance on timeout");
    }

    #[test]
    fn test_wp_panel_location_lookup() {
        assert_eq!(wp_panel_location("Cold Plains"), Some((0, 1)));
        assert_eq!(wp_panel_location("Lut Gholein"), Some((1, 0)));
        assert_eq!(wp_panel_location("Travincal"), Some((2, 7)));
        assert_eq!(wp_panel_location("River of Flame"), Some((3, 2)));
        assert_eq!(wp_panel_location("Harrogath"), Some((4, 0)));
        assert_eq!(wp_panel_location("nonexistent"), None);
    }

    #[test]
    fn test_npc_positions_reasonable() {
        let npcs = [
            ("Akara", 1),
            ("Charsi", 1),
            ("Fara", 2),
            ("Ormus", 3),
            ("Tyrael", 4),
            ("Malah", 5),
        ];
        for (name, act) in npcs {
            let (x, y) = npc_position(name, act);
            assert!(
                x > 0 && x < 800,
                "NPC {} act {} x={} out of bounds",
                name,
                act,
                x
            );
            assert!(
                y > 0 && y < 600,
                "NPC {} act {} y={} out of bounds",
                name,
                act,
                y
            );
        }
    }

    #[test]
    fn test_full_pindle_plan_execution() {
        // Simulate Pindle's simple plan: TownChores → TalkNpc → InteractObject → Wait → Kill → Loot → TP
        let mut exec = make_executor();
        let mut engine = make_engine();

        let plan = vec![
            ScriptStep::TownChores,
            ScriptStep::TalkToNpc {
                npc: "Anya",
                act: 5,
            },
            ScriptStep::InteractObject {
                name: "Anya Portal",
            },
            ScriptStep::WaitForCue {
                cue: VisualCue::LoadingScreenEnd,
                timeout_secs: 15,
            },
            ScriptStep::KillTarget { name: "Pindleskin" },
            ScriptStep::LootArea,
            ScriptStep::TownPortal,
        ];
        exec.load_plan(plan);

        assert_eq!(exec.step_count(), 7);
        assert_eq!(exec.step_index(), 0);

        // Step 0: TownChores — returns None
        let state = town_state();
        assert!(exec.tick(&state, &mut engine).is_none());
        exec.skip_step(); // GameManager handles this
        assert_eq!(exec.step_index(), 1);
    }

    #[test]
    fn test_level_gate_helper() {
        let mut exec = make_executor();
        exec.load_plan(vec![ScriptStep::RequireLevel { min_level: 8 }]);
        assert_eq!(exec.level_gate(), Some(8));

        exec.load_plan(vec![ScriptStep::TownChores]);
        assert_eq!(exec.level_gate(), None);
    }

    #[test]
    fn test_multi_step_advancement() {
        let mut exec = make_executor();
        let mut engine = make_engine();

        exec.load_plan(vec![
            ScriptStep::RequireLevel { min_level: 1 },
            ScriptStep::RequireLevel { min_level: 1 },
            ScriptStep::RequireLevel { min_level: 1 },
            ScriptStep::TownChores,
        ]);

        let mut state = town_state();
        state.char_level = 50;

        // Should chain through all RequireLevel steps until TownChores
        let result = exec.tick(&state, &mut engine);
        assert!(result.is_none()); // Hit TownChores
        assert_eq!(exec.step_index(), 3);
    }
}
