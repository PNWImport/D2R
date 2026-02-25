//! Game lifecycle manager — the kolbot D2BotLead + Locations.js + Town.doChores()
//! equivalent for D2R single-player.
//!
//! Sits above DecisionEngine and orchestrates the full game loop:
//!   OutOfGame → InTown (prep) → Farming → Returning → ExitGame → delay → repeat
//!
//! The DecisionEngine handles frame-by-frame combat/survival. GameManager handles
//! the strategic layer: when to go to town, which town tasks to do, when to
//! exit the game, run counting, and session management.

use super::engine::{Action, Decision, DecisionEngine};
use super::progression::{script_plan, ProgressionEngine, Script, ScriptStep};
use super::quad_cache::QuadCache;
use super::script_executor::ScriptExecutor;
use crate::config::AgentConfig;
use crate::vision::FrameState;
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════
// GAME PHASE STATE MACHINE
// ═══════════════════════════════════════════════════════════════

/// High-level game phase — equivalent to kolbot's OOG location states.
/// For single-player D2R, much simpler than Battle.net (no lobby, no game names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    /// At main menu, character select, or difficulty select
    OutOfGame,
    /// In town, executing town tasks (heal → stash → potions → repair → merc)
    TownPrep,
    /// Walking from town waypoint/exit to farming area
    LeavingTown,
    /// Active farming — DecisionEngine controls combat
    Farming,
    /// Portal back to town (belt low, inventory full, or farming complete)
    Returning,
    /// Saving and exiting game (Esc → Save & Exit)
    ExitingGame,
    /// Waiting between games (humanized delay)
    InterGameDelay,
}

/// Town task sequence — mirrors kolbot's Town.doChores() order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TownTask {
    Heal,
    Identify,
    Stash,
    BuyPotions,
    Repair,
    ReviveMerc,
    Done,
}

impl TownTask {
    /// Next task in the sequence (kolbot order)
    fn next(self) -> Self {
        match self {
            Self::Heal => Self::Identify,
            Self::Identify => Self::Stash,
            Self::Stash => Self::BuyPotions,
            Self::BuyPotions => Self::Repair,
            Self::Repair => Self::ReviveMerc,
            Self::ReviveMerc => Self::Done,
            Self::Done => Self::Done,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// NPC COORDINATES — per-act town NPC screen positions
// kolbot: Town.js lines 15-101 hardcoded coordinates
// Base coordinates at 800×600, scaled to actual resolution via scale_npc_pos().
// ═══════════════════════════════════════════════════════════════

/// Town NPC positions per act (base coordinates at 800×600, scaled at runtime)
struct TownNpcs {
    healer: (i32, i32),
    stash: (i32, i32),
    potion_vendor: (i32, i32),
    repair_vendor: (i32, i32),
    merc_revive: (i32, i32),
    identify: (i32, i32), // Cain / Deckard
    _waypoint: (i32, i32),
}

/// Scale a base-800×600 NPC position to the actual frame resolution.
fn scale_npc_pos(pos: (i32, i32), fw: u16, fh: u16) -> (i32, i32) {
    (
        (pos.0 as f32 * fw as f32 / 800.0) as i32,
        (pos.1 as f32 * fh as f32 / 600.0) as i32,
    )
}

/// Scale all positions in a TownNpcs set to the actual frame resolution.
fn scale_town_npcs(npcs: TownNpcs, fw: u16, fh: u16) -> TownNpcs {
    TownNpcs {
        healer: scale_npc_pos(npcs.healer, fw, fh),
        stash: scale_npc_pos(npcs.stash, fw, fh),
        potion_vendor: scale_npc_pos(npcs.potion_vendor, fw, fh),
        repair_vendor: scale_npc_pos(npcs.repair_vendor, fw, fh),
        merc_revive: scale_npc_pos(npcs.merc_revive, fw, fh),
        identify: scale_npc_pos(npcs.identify, fw, fh),
        _waypoint: scale_npc_pos(npcs._waypoint, fw, fh),
    }
}

fn npcs_for_act(act: u8) -> TownNpcs {
    match act {
        1 => TownNpcs {
            healer: (155, 72),         // Akara
            stash: (127, 237),         // Stash (camp center)
            potion_vendor: (155, 72),  // Akara (same)
            repair_vendor: (257, 209), // Charsi
            merc_revive: (466, 236),   // Kashya
            identify: (155, 72),       // Akara (or Cain if rescued)
            _waypoint: (120, 280),     // WP in Rogue Camp
        },
        2 => TownNpcs {
            healer: (260, 142),        // Fara
            stash: (230, 290),         // Stash
            potion_vendor: (196, 93),  // Drognan
            repair_vendor: (260, 142), // Fara (same)
            merc_revive: (457, 218),   // Greiz
            identify: (196, 93),       // Drognan (or Cain)
            _waypoint: (264, 288),     // WP in Lut Gholein
        },
        3 => TownNpcs {
            healer: (307, 170),        // Ormus
            stash: (166, 310),         // Stash
            potion_vendor: (307, 170), // Ormus
            repair_vendor: (226, 63),  // Hratli
            merc_revive: (408, 95),    // Asheara
            identify: (307, 170),      // Ormus (or Cain)
            _waypoint: (229, 306),     // WP in Kurast Docks
        },
        4 => TownNpcs {
            healer: (152, 107),        // Jamella
            stash: (186, 246),         // Stash
            potion_vendor: (152, 107), // Jamella
            repair_vendor: (181, 155), // Halbu
            merc_revive: (152, 107),   // Tyrael (act 4 = Tyrael)
            identify: (152, 107),      // Jamella (or Cain)
            _waypoint: (158, 277),     // WP in Pandemonium Fortress
        },
        _ => TownNpcs {
            // Act 5
            healer: (328, 63),         // Malah
            stash: (306, 266),         // Stash
            potion_vendor: (328, 63),  // Malah
            repair_vendor: (135, 142), // Larzuk
            merc_revive: (458, 147),   // Qual-Kehk
            identify: (385, 154),      // Anya (or Cain)
            _waypoint: (210, 172),     // WP in Harrogath
        },
    }
}

// ═══════════════════════════════════════════════════════════════
// GAME MANAGER
// ═══════════════════════════════════════════════════════════════

/// A point of interest relayed from the maphack overlay host.
/// Coordinates are in D2R world (game) space, not screen pixels.
#[derive(Debug, Clone)]
pub struct MapPoi {
    pub x:           i32,
    pub y:           i32,
    pub poi_type:    String, // "Exit", "Waypoint", "Staircase", "Shrine", etc.
    pub label:       String,
    pub target_area: Option<u32>, // For exits: which area they lead to
}

pub struct GameManager {
    config: AgentConfig,
    engine: DecisionEngine,
    /// QuadCache — warmed at startup, re-indexed on config reload.
    /// Lane 2: pre-loaded farm run scripts (O(1) run lookup)
    /// Lane 3: threshold bins (owned by engine; mirrored here for run-level logic)
    /// Lane 4: hot-pattern telemetry for LLM strategic wrapper
    cache: QuadCache,
    rng: StdRng,

    // Phase tracking
    phase: GamePhase,
    last_phase_change: Instant,
    in_town_frames: u8, // Counter for stable in_town detection
    town_task: TownTask,
    town_task_started: Instant,
    town_npc_walk_time: Duration,

    // Game lifecycle
    game_start: Instant,
    game_count: u32,
    run_index: usize,    // Current position in farming.sequence
    runs_this_game: u32, // How many farming runs completed this game
    last_game_exit: Instant,

    // Session tracking
    _session_start: Instant,
    total_games: u32,
    total_chickens: u32,

    // OOG navigation state
    oog_click_cooldown: Instant,

    // Exit sequence state
    exit_step: u8, // 0=esc, 1=wait, 2=click_save

    // ─── Progression / Leveling ──────────────────────────────
    /// Quest progression engine (None = farming-only mode, no quest tracking).
    progression: Option<ProgressionEngine>,
    /// Currently executing script (selected by progression engine).
    current_script: Option<Script>,
    /// Steps remaining for the current script (legacy, kept for current_step/check_level_gate).
    script_steps: Vec<ScriptStep>,
    /// Index into script_steps (legacy).
    script_step_index: usize,
    /// When the current script step started (for timeouts).
    step_started: Instant,

    // ─── Script Executor ──────────────────────────────────────
    /// Drives ScriptStep plans into per-frame Actions.
    executor: ScriptExecutor,

    // ─── Map State (relayed from maphack overlay host) ────────
    /// Real area ID read from D2R memory. 0 = unknown.
    map_area_id: u32,
    /// Real area name — replaces vision's "_banner_detected" placeholder.
    map_area_name: String,
    /// Player world coordinates (game-space, updated each map poll).
    player_world_x: i32,
    player_world_y: i32,
    /// Map seed from D2R memory (used to generate collision/POI data).
    map_seed: u32,
    /// Difficulty: 0=Normal, 1=Nightmare, 2=Hell.
    map_difficulty: u8,
    /// Points of interest for the current area (exits, WPs, stairs).
    map_pois: Vec<MapPoi>,
}

impl GameManager {
    pub fn new(config: AgentConfig) -> Self {
        let engine = DecisionEngine::new(config.clone());
        let cache = QuadCache::warm(&config);
        let now = Instant::now();
        Self {
            config,
            engine,
            cache,
            rng: StdRng::from_entropy(),
            phase: GamePhase::OutOfGame,
            last_phase_change: now,
            in_town_frames: 0,
            town_task: TownTask::Heal,
            town_task_started: now,
            town_npc_walk_time: Duration::from_millis(800),
            game_start: now,
            game_count: 0,
            run_index: 0,
            runs_this_game: 0,
            last_game_exit: now,
            _session_start: now,
            total_games: 0,
            total_chickens: 0,
            oog_click_cooldown: now,
            exit_step: 0,
            progression: None,
            current_script: None,
            script_steps: Vec::new(),
            script_step_index: 0,
            step_started: now,
            executor: ScriptExecutor::new(),
            map_area_id: 0,
            map_area_name: String::new(),
            player_world_x: 0,
            player_world_y: 0,
            map_seed: 0,
            map_difficulty: 0,
            map_pois: Vec::new(),
        }
    }

    /// Create a GameManager with quest progression enabled (leveling mode).
    /// `state_path` is the path to the quest state JSON file.
    pub fn with_progression(config: AgentConfig, state_path: std::path::PathBuf) -> Self {
        let class = config.character_class.clone();
        let mut mgr = Self::new(config);
        mgr.progression = Some(ProgressionEngine::new(class, state_path));
        mgr
    }

    /// Apply map state relayed from the maphack overlay host.
    /// Called whenever Chrome forwards a `read_state` response to the vision agent.
    pub fn apply_map_state(
        &mut self,
        area_id:    u32,
        area_name:  String,
        player_x:   i32,
        player_y:   i32,
        map_seed:   u32,
        difficulty: u8,
        pois:       &serde_json::Value,
    ) {
        self.map_area_id    = area_id;
        self.player_world_x = player_x;
        self.player_world_y = player_y;
        self.map_seed       = map_seed;
        self.map_difficulty = difficulty;

        if !area_name.is_empty() {
            self.map_area_name = area_name;
        }

        if let Some(arr) = pois.as_array() {
            self.map_pois = arr
                .iter()
                .filter_map(|p| {
                    Some(MapPoi {
                        x:           p.get("x")?.as_i64()? as i32,
                        y:           p.get("y")?.as_i64()? as i32,
                        poi_type:    p.get("poi_type")?.as_str()?.to_string(),
                        label:       p.get("label").and_then(|l| l.as_str()).unwrap_or("").to_string(),
                        target_area: p.get("target_area").and_then(|t| t.as_u64()).map(|t| t as u32),
                    })
                })
                .collect();
        }

        tracing::debug!(
            "map_state: area_id={} name={:?} player=({},{}) seed={:#x} pois={}",
            self.map_area_id, self.map_area_name,
            self.player_world_x, self.player_world_y,
            self.map_seed, self.map_pois.len()
        );
    }

    /// Convert D2R world coordinates to screen pixel coordinates.
    ///
    /// D2R uses an isometric projection. The player is always rendered at
    /// roughly the screen center (char_screen_x/y from FrameState).
    /// Each world unit maps to ~16 horizontal and ~8 vertical screen pixels
    /// in the default view — calibrate these if the bot overshoots/undershoots.
    pub fn world_to_screen(&self, world_x: i32, world_y: i32, frame: &crate::vision::FrameState) -> (i32, i32) {
        let dx = world_x - self.player_world_x;
        let dy = world_y - self.player_world_y;
        let cx = frame.char_screen_x as i32;
        let cy = frame.char_screen_y as i32;
        // Isometric projection: x-axis goes right-down, y-axis goes left-down
        let screen_x = cx + (dx - dy) * 16;
        let screen_y = cy + (dx + dy) * 8;
        (screen_x, screen_y)
    }

    /// Find the nearest exit/staircase POI, optionally filtering by target area.
    pub fn next_exit_poi(&self, target_area: Option<u32>) -> Option<&MapPoi> {
        self.map_pois.iter().find(|p| {
            (p.poi_type == "Exit" || p.poi_type == "Staircase")
                && target_area.map_or(true, |ta| p.target_area == Some(ta))
        })
    }

    /// Find the waypoint POI for the current area, if any.
    pub fn waypoint_poi(&self) -> Option<&MapPoi> {
        self.map_pois.iter().find(|p| p.poi_type == "Waypoint")
    }

    /// Best-effort area name: maphack name takes priority, falls back to vision banner.
    pub fn current_area_name<'a>(&'a self, frame: &'a crate::vision::FrameState) -> &'a str {
        if !self.map_area_name.is_empty() {
            return &self.map_area_name;
        }
        let banner = frame.area_name_str();
        if banner != "_banner_detected" {
            return banner;
        }
        ""
    }

    /// Ensure the minimap is open in "mini" mode before navigation.
    ///
    /// D2R Tab cycles: no map → mini map (top-right circle) → full-screen map.
    /// Returns a Tab-key Decision if the minimap is not currently visible.
    /// Returns None if minimap is already open — caller should proceed with navigation.
    pub fn ensure_minimap_open(&self, frame: &crate::vision::FrameState) -> Option<Decision> {
        if frame.minimap_visible {
            return None; // Already open — caller proceeds
        }
        // Not visible: press Tab to cycle toward mini mode.
        // One Tab toggles: off→mini, mini→full, full→off.
        // We'll press Tab once and let the next frame's detect_minimap_visible()
        // confirm which state we're in. The caller should check again next frame.
        Some(Decision {
            action: Action::RecastBuff { key: '\t' },
            delay: std::time::Duration::from_millis(200),
            priority: 1,
            reason: "minimap: pressing Tab to open mini map",
        })
    }

    /// Navigate toward the minimap exit chevron (pure vision, no memory reads).
    ///
    /// The minimap center = player position. The exit marker offset tells us which
    /// direction to move. We return a screen-pixel teleport target (incremental
    /// step — call repeatedly until the exit loads).
    ///
    /// Scale: minimap radius ≈ 95px shows ~240 tiles. Screen ≈ 40 tiles wide at
    /// 1280×720 → scale ≈ 80px/minimap-px. We use scale=20 (25% per step).
    pub fn navigate_toward_minimap_exit(&self, frame: &crate::vision::FrameState) -> Option<(i32, i32)> {
        if !frame.minimap_visible || !frame.minimap_has_exit { return None; }

        let fw = frame.frame_width  as f32;
        let fh = frame.frame_height as f32;
        let mm_cx = (fw * 0.898) as i32;
        let mm_cy = (fh * 0.159) as i32;

        let dx = frame.minimap_exit_screen_x as i32 - mm_cx;
        let dy = frame.minimap_exit_screen_y as i32 - mm_cy;
        if dx.abs() < 5 && dy.abs() < 5 { return None; } // spurious center hit

        let target_x = (frame.char_screen_x as i32 + dx * 20).clamp(80, fw as i32 - 80);
        let target_y = (frame.char_screen_y as i32 + dy * 20).clamp(80, fh as i32 - 80);
        Some((target_x, target_y))
    }

    /// Navigate toward the minimap waypoint marker (pure vision).
    pub fn navigate_toward_minimap_waypoint(&self, frame: &crate::vision::FrameState) -> Option<(i32, i32)> {
        if !frame.minimap_visible || !frame.minimap_has_waypoint { return None; }

        let fw = frame.frame_width  as f32;
        let fh = frame.frame_height as f32;
        let mm_cx = (fw * 0.898) as i32;
        let mm_cy = (fh * 0.159) as i32;

        let dx = frame.minimap_wp_screen_x as i32 - mm_cx;
        let dy = frame.minimap_wp_screen_y as i32 - mm_cy;
        if dx.abs() < 5 && dy.abs() < 5 { return None; }

        let target_x = (frame.char_screen_x as i32 + dx * 20).clamp(80, fw as i32 - 80);
        let target_y = (frame.char_screen_y as i32 + dy * 20).clamp(80, fh as i32 - 80);
        Some((target_x, target_y))
    }

    /// Current game phase
    pub fn phase(&self) -> GamePhase {
        self.phase
    }

    /// Access the inner decision engine (for stats, reload, etc.)
    pub fn engine(&self) -> &DecisionEngine {
        &self.engine
    }

    /// Mutable access to inner engine
    pub fn engine_mut(&mut self) -> &mut DecisionEngine {
        &mut self.engine
    }

    /// Reload config for both manager and engine
    pub fn reload_config(&mut self, config: AgentConfig) {
        // Lane 3: re-flatten survival thresholds immediately
        self.cache.reload_thresholds(&config);
        // Lane 2: re-index run sequence if farming.sequence may have changed
        self.cache.rewarm_runs(&config);
        self.engine.reload_config(config.clone());
        self.config = config;
    }

    /// Total games completed this session
    pub fn total_games(&self) -> u32 {
        self.total_games
    }

    /// Current game number
    pub fn game_count(&self) -> u32 {
        self.game_count
    }

    // ─── Main Decision Dispatch ──────────────────────────────────

    /// Top-level decision: delegates to the right handler based on game phase.
    /// This replaces the direct `engine.decide()` call in the main loop.
    pub fn decide(&mut self, state: &FrameState) -> Decision {
        // Auto-detect phase transitions from vision state
        self.detect_phase_transitions(state);

        match self.phase {
            GamePhase::OutOfGame => self.handle_oog(state),
            GamePhase::TownPrep => self.handle_town(state),
            GamePhase::LeavingTown => self.handle_leaving_town(state),
            GamePhase::Farming => self.handle_farming(state),
            GamePhase::Returning => self.handle_returning(state),
            GamePhase::ExitingGame => self.handle_exit(state),
            GamePhase::InterGameDelay => self.handle_inter_game_delay(),
        }
    }

    // ─── Phase Transition Detection ──────────────────────────────

    fn detect_phase_transitions(&mut self, state: &FrameState) {
        // Stable in_town detection: require 3+ consecutive frames before trusting it
        // This prevents flickering vision from causing rapid phase cycling
        if state.in_town {
            self.in_town_frames = self.in_town_frames.saturating_add(1);
        } else {
            self.in_town_frames = 0;
        }
        let stable_in_town = self.in_town_frames >= 3;

        // Hysteresis: prevent rapid phase cycling due to flickering vision state.
        // Require at least 1.5s between phase changes (except for critical transitions).
        let min_phase_duration = Duration::from_millis(1500);
        let time_in_phase = self.last_phase_change.elapsed();

        match self.phase {
            GamePhase::OutOfGame => {
                if !state.at_menu && !state.loading_screen {
                    if stable_in_town {
                        // Normal case: loaded into game, standing in town
                        self.phase = GamePhase::TownPrep;
                        self.last_phase_change = Instant::now();
                        self.town_task = TownTask::Heal;
                        self.game_start = Instant::now();
                        self.game_count += 1;
                        self.total_games += 1;
                        self.runs_this_game = 0;
                        self.run_index = 0;
                        self.on_game_start_progression();
                        tracing::info!("Game #{} started — entering town prep", self.game_count);
                    } else if self.in_town_frames == 0
                        && time_in_phase > Duration::from_secs(2)
                    {
                        // Bot started mid-dungeon (or returned here erroneously).
                        // After 2 stable seconds of "in-game, not in town", go to Farming.
                        self.phase = GamePhase::Farming;
                        self.last_phase_change = Instant::now();
                        self.game_start = Instant::now();
                        self.game_count += 1;
                        self.total_games += 1;
                        self.runs_this_game = 0;
                        self.run_index = 0;
                        self.on_game_start_progression();
                        tracing::info!(
                            "Game #{} detected mid-dungeon — entering Farming directly",
                            self.game_count
                        );
                    }
                }
            }
            GamePhase::TownPrep => {
                // If we somehow left town during prep (chicken, etc.)
                if self.in_town_frames == 0 && !state.at_menu && !state.loading_screen && time_in_phase > min_phase_duration {
                    self.phase = GamePhase::Farming;
                    self.last_phase_change = Instant::now();
                }
            }
            GamePhase::LeavingTown => {
                // Arrived at farming area
                if self.in_town_frames == 0 && !state.loading_screen && time_in_phase > Duration::from_millis(500) {
                    self.phase = GamePhase::Farming;
                    self.last_phase_change = Instant::now();
                    tracing::info!("Arrived at farming area");
                }
            }
            GamePhase::Farming => {
                // Returned to town (TP, chicken, or walked back)
                // CRITICAL: Only transition if in_town is stable AND we've been farming for at least 1.5s
                if stable_in_town && time_in_phase > min_phase_duration {
                    self.phase = GamePhase::Returning;
                    self.last_phase_change = Instant::now();
                    tracing::info!("Back in town — checking if more runs needed (stable detection)");
                }
                // Got kicked to menu (disconnect, crash) — immediate transition
                if state.at_menu {
                    self.phase = GamePhase::OutOfGame;
                    self.last_phase_change = Instant::now();
                }
            }
            GamePhase::Returning => {
                // Still in town, decide what to do
                if state.at_menu {
                    self.phase = GamePhase::OutOfGame;
                    self.last_phase_change = Instant::now();
                }
            }
            GamePhase::ExitingGame => {
                if state.at_menu {
                    self.phase = GamePhase::InterGameDelay;
                    self.last_phase_change = Instant::now();
                    self.last_game_exit = Instant::now();
                    self.exit_step = 0;
                    self.on_game_end_progression();
                    tracing::info!("Game exited — starting inter-game delay");
                }
            }
            GamePhase::InterGameDelay => {
                // Timer-based, no vision trigger needed
            }
        }
    }

    // ─── Out of Game (OOG) Handler ───────────────────────────────
    // Equivalent to kolbot Locations.js — navigates menus to start a game.
    // For single player: Main Menu → Play → Offline → Select Char → Difficulty

    fn handle_oog(&mut self, state: &FrameState) -> Decision {
        let now = Instant::now();

        // If game HUD is visible (orbs rendering), we're inside an active game session —
        // NOT at a menu. Stop clicking and let detect_phase_transitions find the phase.
        if !state.at_menu && !state.loading_screen {
            return Decision {
                action: Action::Wait,
                delay: Duration::from_millis(200),
                priority: 0,
                reason: "oog: game HUD visible, waiting for phase detection",
            };
        }

        // Rate-limit menu clicks to avoid double-clicking
        if now.duration_since(self.oog_click_cooldown) < Duration::from_millis(1500) {
            return Decision {
                action: Action::Wait,
                delay: Duration::from_millis(500),
                priority: 0,
                reason: "oog: waiting for menu transition",
            };
        }

        if state.loading_screen {
            return Decision {
                action: Action::Wait,
                delay: Duration::from_millis(500),
                priority: 0,
                reason: "oog: loading screen",
            };
        }

        // Click through menus — the vision pipeline will tell us which screen
        // we're on via at_menu + phash. For now, assume a simple sequence:
        // We click center-ish areas that correspond to the standard SP flow.
        self.oog_click_cooldown = now;

        // Generic "advance through menu" click at the primary action area
        // The exact coordinates depend on which menu screen we're on.
        // Vision pipeline will eventually provide more granular screen detection.
        // Scale from 800×600 base coords to actual frame resolution.
        let fw = state.frame_width;
        let fh = state.frame_height;
        Decision {
            action: Action::MoveTo {
                screen_x: (400.0 * fw as f32 / 800.0) as i32,
                screen_y: (340.0 * fh as f32 / 600.0) as i32,
            },
            delay: Duration::from_millis(self.rng.gen_range(300..800)),
            priority: 0,
            reason: "oog: advancing through menu",
        }
    }

    // ─── Town Prep Handler ───────────────────────────────────────
    // Equivalent to kolbot Town.doChores() — sequential NPC visits.

    fn handle_town(&mut self, state: &FrameState) -> Decision {
        let npcs = scale_town_npcs(
            npcs_for_act(state.current_act),
            state.frame_width,
            state.frame_height,
        );

        // Check if current task has had enough time to complete (walk + interact)
        let task_elapsed = self.town_task_started.elapsed();

        match self.town_task {
            TownTask::Heal => {
                if task_elapsed < self.town_npc_walk_time {
                    return self.walk_to(npcs.healer, "town: walking to healer");
                }
                // Click healer NPC to interact
                if task_elapsed < self.town_npc_walk_time + Duration::from_millis(500) {
                    return self.click_at(npcs.healer, "town: interacting with healer");
                }
                self.advance_town_task();
                self.handle_town(state) // recurse to next task
            }
            TownTask::Identify => {
                // Skip if Cain ID is disabled
                if !self.config.loot.cain_id {
                    self.advance_town_task();
                    return self.handle_town(state);
                }
                if task_elapsed < self.town_npc_walk_time {
                    return self.walk_to(npcs.identify, "town: walking to Cain");
                }
                if task_elapsed < self.town_npc_walk_time + Duration::from_millis(500) {
                    return self.click_at(npcs.identify, "town: identifying items");
                }
                self.advance_town_task();
                self.handle_town(state)
            }
            TownTask::Stash => {
                if task_elapsed < self.town_npc_walk_time {
                    return self.walk_to(npcs.stash, "town: walking to stash");
                }
                if task_elapsed < self.town_npc_walk_time + Duration::from_millis(500) {
                    return self.click_at(npcs.stash, "town: opening stash");
                }
                // Stash interaction takes longer — deposit items
                if task_elapsed < self.town_npc_walk_time + Duration::from_secs(3) {
                    return Decision {
                        action: Action::Wait,
                        delay: Duration::from_millis(200),
                        priority: 1,
                        reason: "town: depositing items",
                    };
                }
                // Close stash (Esc)
                self.advance_town_task();
                let cx = (state.frame_width / 2) as i32;
                let cy = (state.frame_height / 2) as i32;
                Decision {
                    action: Action::CastSkill {
                        key: '\x1b', // Escape
                        screen_x: cx,
                        screen_y: cy,
                    },
                    delay: Duration::from_millis(200),
                    priority: 1,
                    reason: "town: closing stash",
                }
            }
            TownTask::BuyPotions => {
                if task_elapsed < self.town_npc_walk_time {
                    return self.walk_to(npcs.potion_vendor, "town: walking to potion vendor");
                }
                if task_elapsed < self.town_npc_walk_time + Duration::from_millis(500) {
                    return self.click_at(npcs.potion_vendor, "town: buying potions");
                }
                // Potion buying interaction
                if task_elapsed < self.town_npc_walk_time + Duration::from_secs(2) {
                    return Decision {
                        action: Action::Wait,
                        delay: Duration::from_millis(200),
                        priority: 1,
                        reason: "town: purchasing potions",
                    };
                }
                self.advance_town_task();
                self.handle_town(state)
            }
            TownTask::Repair => {
                if task_elapsed < self.town_npc_walk_time {
                    return self.walk_to(npcs.repair_vendor, "town: walking to repair vendor");
                }
                if task_elapsed < self.town_npc_walk_time + Duration::from_millis(500) {
                    return self.click_at(npcs.repair_vendor, "town: repairing items");
                }
                if task_elapsed < self.town_npc_walk_time + Duration::from_secs(1) {
                    return Decision {
                        action: Action::Wait,
                        delay: Duration::from_millis(200),
                        priority: 1,
                        reason: "town: repair in progress",
                    };
                }
                self.advance_town_task();
                self.handle_town(state)
            }
            TownTask::ReviveMerc => {
                // Skip if merc is alive or merc not used
                if !self.config.merc.use_merc || state.merc_alive {
                    self.advance_town_task();
                    return self.handle_town(state);
                }
                if task_elapsed < self.town_npc_walk_time {
                    return self.walk_to(npcs.merc_revive, "town: walking to merc NPC");
                }
                if task_elapsed < self.town_npc_walk_time + Duration::from_millis(500) {
                    return self.click_at(npcs.merc_revive, "town: reviving mercenary");
                }
                if task_elapsed < self.town_npc_walk_time + Duration::from_secs(1) {
                    return Decision {
                        action: Action::Wait,
                        delay: Duration::from_millis(200),
                        priority: 1,
                        reason: "town: merc revive in progress",
                    };
                }
                self.advance_town_task();
                self.handle_town(state)
            }
            TownTask::Done => {
                // All town tasks complete.
                // If no script is loaded yet (first town visit), select one.
                if self.progression.is_some()
                    && self.current_script.is_none()
                    && !self.select_next_script()
                {
                    // No more scripts — exit game
                    self.phase = GamePhase::ExitingGame;
                    self.last_phase_change = Instant::now();
                    return Decision {
                        action: Action::Wait,
                        delay: Duration::from_millis(500),
                        priority: 0,
                        reason: "town: all scripts done, exiting",
                    };
                }

                // Resume executor — it will handle WP navigation, walking, etc.
                tracing::info!("Town prep complete — resuming script executor");
                self.phase = GamePhase::LeavingTown;
                self.last_phase_change = Instant::now();
                // First tick of the executor from LeavingTown
                self.drive_executor(state)
            }
        }
    }

    fn advance_town_task(&mut self) {
        self.town_task = self.town_task.next();
        self.town_task_started = Instant::now();
    }

    // ─── Leaving Town / Farming Handler ────────────────────────────
    // Both phases are now driven by the ScriptExecutor. The executor
    // interprets the ScriptStep plan loaded by select_next_script().
    // LeavingTown transitions to Farming once we leave town (handled
    // by detect_phase_transitions). Both phases tick the executor.

    fn handle_leaving_town(&mut self, state: &FrameState) -> Decision {
        // Drive the executor — it handles waypoint clicks, walking, etc.
        self.drive_executor(state)
    }

    fn handle_farming(&mut self, state: &FrameState) -> Decision {
        // Update character level from vision
        if state.char_level > 0 {
            self.update_level_from_vision(state.char_level);
        }

        // Check for quest completion visual cue
        if state.quest_complete_banner {
            self.on_quest_complete_visual();
        }

        // Check if we should return to town (belt empty, inventory full, merc dead)
        if self.should_return_to_town(state) {
            tracing::info!("Town trigger hit — casting TP");
            self.phase = GamePhase::Returning;
            self.last_phase_change = Instant::now();
            return Decision {
                action: Action::TownPortal,
                delay: Duration::from_millis(100),
                priority: 4,
                reason: "farming: returning to town (trigger hit)",
            };
        }

        // Check max game time
        let max_mins = self.config.farming.max_game_time_mins;
        if max_mins > 0 && self.game_start.elapsed() > Duration::from_secs(max_mins as u64 * 60) {
            tracing::info!("Max game time reached — exiting");
            self.phase = GamePhase::ExitingGame;
            self.last_phase_change = Instant::now();
            return Decision {
                action: Action::Wait,
                delay: Duration::ZERO,
                priority: 0,
                reason: "farming: max game time reached",
            };
        }

        // ─── Survival always takes priority over script execution ───
        // Check immediate survival needs BEFORE executor (chicken, potions, etc.)
        if state.in_combat {
            let survival = self.engine.decide(state);
            match &survival.action {
                Action::ChickenQuit => {
                    self.total_chickens += 1;
                    self.phase = GamePhase::ExitingGame;
                    self.last_phase_change = Instant::now();
                    return survival;
                }
                Action::DrinkPotion { .. } | Action::TownPortal | Action::Dodge { .. } => {
                    return survival;
                }
                _ => {} // Non-survival action — let executor drive
            }
        }

        // ─── Script executor drives navigation + combat ─────────────
        self.drive_executor(state)
    }

    /// Tick the ScriptExecutor and handle its output.
    /// Called by both handle_leaving_town and handle_farming.
    fn drive_executor(&mut self, state: &FrameState) -> Decision {
        // If executor has no plan loaded or is done, fall back
        if self.executor.is_done() {
            if self.current_script.is_some() {
                // Script complete — mark done and select next
                self.complete_current_script();
                // Try to load next script
                if !self.select_next_script() {
                    self.phase = GamePhase::ExitingGame;
                    self.last_phase_change = Instant::now();
                    return Decision {
                        action: Action::Wait,
                        delay: Duration::from_millis(500),
                        priority: 0,
                        reason: "executor: all scripts done, exiting",
                    };
                }
            } else {
                // No progression engine — fall back to DecisionEngine
                let decision = self.engine.decide(state);
                if matches!(decision.action, Action::ChickenQuit) {
                    self.total_chickens += 1;
                    self.phase = GamePhase::ExitingGame;
                    self.last_phase_change = Instant::now();
                }
                return decision;
            }
        }

        // Check if current step is TownChores — if so, transition to TownPrep
        if let Some(ScriptStep::TownChores) = self.executor.current_step() {
            self.executor.skip_step(); // Mark TownChores as handled
            self.phase = GamePhase::TownPrep;
            self.last_phase_change = Instant::now();
            self.town_task = TownTask::Heal;
            self.town_task_started = Instant::now();
            tracing::info!("Script step: TownChores — entering town prep");
            return Decision {
                action: Action::Wait,
                delay: Duration::from_millis(200),
                priority: 1,
                reason: "executor: starting town chores",
            };
        }

        // Check level gate
        if let Some(min_level) = self.executor.level_gate() {
            if state.char_level < min_level {
                tracing::info!(
                    "Level gate: need {}, have {} — retry next game",
                    min_level,
                    state.char_level
                );
                if let Some(script) = self.current_script {
                    if let Some(ref mut progression) = self.progression {
                        progression.retry_next_game(script);
                    }
                }
                self.current_script = None;
                // Try to select next script
                if !self.select_next_script() {
                    self.phase = GamePhase::ExitingGame;
                    self.last_phase_change = Instant::now();
                }
                return Decision {
                    action: Action::Wait,
                    delay: Duration::from_millis(500),
                    priority: 0,
                    reason: "executor: level gate failed",
                };
            }
        }

        // Check RetryNextGame step
        if let Some(ScriptStep::RetryNextGame) = self.executor.current_step() {
            if let Some(script) = self.current_script {
                if let Some(ref mut progression) = self.progression {
                    progression.retry_next_game(script);
                }
            }
            self.executor.skip_step();
            return Decision {
                action: Action::Wait,
                delay: Duration::from_millis(200),
                priority: 0,
                reason: "executor: retry next game",
            };
        }

        // Tick the executor
        match self.executor.tick(state, &mut self.engine) {
            Some(decision) => decision,
            None => {
                // Executor returned None — usually means TownChores or level gate
                // which we already handle above. Shouldn't happen, but safe fallback.
                Decision {
                    action: Action::Wait,
                    delay: Duration::from_millis(200),
                    priority: 5,
                    reason: "executor: no action (fallback)",
                }
            }
        }
    }

    fn should_return_to_town(&self, state: &FrameState) -> bool {
        // Don't trigger during combat
        if state.in_combat && state.enemy_count > 0 {
            return false;
        }

        let triggers = &self.config.town.go_to_town_triggers;

        // Belt potions low
        let min_pots: u8 = state.belt_columns.iter().sum();
        if min_pots < triggers.belt_potions_below * 4 {
            return true;
        }

        // Inventory full
        if state.inventory_full && triggers.inventory_slots_below > 0 {
            return true;
        }

        // Merc dead
        if triggers.merc_dead && self.config.merc.use_merc && !state.merc_alive {
            return true;
        }

        false
    }

    // ─── Returning to Town Handler ───────────────────────────────
    // We're back in town via TP. Run town chores, then decide: more runs or exit.

    fn handle_returning(&mut self, state: &FrameState) -> Decision {
        self.runs_this_game += 1;

        // If executor still has steps (TP back to town mid-script is normal),
        // resume the executor — it handles talk-to-NPC, stash, next script, etc.
        if !self.executor.is_done() {
            // The script still has steps to run (e.g. "TalkToNpc" after a boss).
            // Check if the next step is TownChores
            if let Some(ScriptStep::TownChores) = self.executor.current_step() {
                // Run town chores first, then resume
                self.executor.skip_step();
                self.phase = GamePhase::TownPrep;
                self.last_phase_change = Instant::now();
                self.town_task = TownTask::Heal;
                self.town_task_started = Instant::now();
                tracing::info!("Back in town — running script TownChores");
                return self.handle_town(state);
            }

            // Resume executor directly (e.g. for TalkToNpc steps in town)
            self.phase = GamePhase::LeavingTown;
            self.last_phase_change = Instant::now();
            tracing::info!("Back in town — resuming script executor");
            return self.drive_executor(state);
        }

        // Executor done — script complete. Try next script.
        self.complete_current_script();

        // Check farming sequence (legacy config mode)
        let sequence_complete = if self.progression.is_some() {
            // Progression mode: let select_next_script decide
            if !self.select_next_script() {
                true // No more scripts
            } else {
                false // New script loaded
            }
        } else if self.config.farming.sequence.is_empty() {
            true // No explicit sequence — one run per game
        } else {
            self.run_index >= self.config.farming.sequence.len()
        };

        if sequence_complete {
            tracing::info!(
                "Farming sequence complete ({} runs) — exiting game",
                self.runs_this_game
            );
            self.phase = GamePhase::ExitingGame;
            self.last_phase_change = Instant::now();
            return Decision {
                action: Action::Wait,
                delay: Duration::from_millis(500),
                priority: 0,
                reason: "returning: sequence complete, will exit",
            };
        }

        // More runs/scripts to do — start town prep
        self.phase = GamePhase::TownPrep;
        self.last_phase_change = Instant::now();
        self.town_task = TownTask::Heal;
        self.town_task_started = Instant::now();
        self.run_index += 1;

        tracing::info!(
            "Run {} complete — starting town prep for next run",
            self.runs_this_game
        );
        self.handle_town(state)
    }

    // ─── Exit Game Handler ───────────────────────────────────────
    // Equivalent to kolbot's game exit sequence: Esc → Save & Exit

    fn handle_exit(&mut self, state: &FrameState) -> Decision {
        let fw = state.frame_width;
        let fh = state.frame_height;
        match self.exit_step {
            0 => {
                self.exit_step = 1;
                // Press Escape to open menu
                Decision {
                    action: Action::CastSkill {
                        key: '\x1b',
                        screen_x: (fw / 2) as i32,
                        screen_y: (fh / 2) as i32,
                    },
                    delay: Duration::from_millis(300),
                    priority: 0,
                    reason: "exit: pressing escape",
                }
            }
            1 => {
                self.exit_step = 2;
                // Click "Save and Exit Game" button
                // Scale from 800×600 base coords to actual frame resolution
                Decision {
                    action: Action::MoveTo {
                        screen_x: (400.0 * fw as f32 / 800.0) as i32,
                        screen_y: (280.0 * fh as f32 / 600.0) as i32,
                    },
                    delay: Duration::from_millis(500),
                    priority: 0,
                    reason: "exit: clicking save and exit",
                }
            }
            _ => {
                // Wait for menu detection to trigger phase transition
                Decision {
                    action: Action::Wait,
                    delay: Duration::from_secs(1),
                    priority: 0,
                    reason: "exit: waiting for menu",
                }
            }
        }
    }

    // ─── Inter-Game Delay Handler ────────────────────────────────
    // Humanized delay between games. Kolbot: Starter.Config.MinGameTime

    fn handle_inter_game_delay(&mut self) -> Decision {
        let min_delay = self.config.farming.min_game_time_secs.max(5) as u64;
        let jitter = self.rng.gen_range(0..min_delay / 2 + 1);
        let total_delay = Duration::from_secs(min_delay + jitter);

        if self.last_game_exit.elapsed() >= total_delay {
            tracing::info!("Inter-game delay complete — starting new game");
            self.phase = GamePhase::OutOfGame;
            self.last_phase_change = Instant::now();
            return Decision {
                action: Action::Wait,
                delay: Duration::ZERO,
                priority: 0,
                reason: "delay: complete, creating new game",
            };
        }

        Decision {
            action: Action::Wait,
            delay: Duration::from_secs(1),
            priority: 0,
            reason: "delay: waiting between games",
        }
    }

    // ─── Helpers ─────────────────────────────────────────────────

    fn walk_to(&mut self, pos: (i32, i32), reason: &'static str) -> Decision {
        let jitter_x = self.rng.gen_range(-8i32..=8);
        let jitter_y = self.rng.gen_range(-8i32..=8);
        Decision {
            action: Action::MoveTo {
                screen_x: pos.0 + jitter_x,
                screen_y: pos.1 + jitter_y,
            },
            delay: Duration::from_millis(self.rng.gen_range(200..500)),
            priority: 1,
            reason,
        }
    }

    fn click_at(&mut self, pos: (i32, i32), reason: &'static str) -> Decision {
        let jitter_x = self.rng.gen_range(-5i32..=5);
        let jitter_y = self.rng.gen_range(-5i32..=5);
        Decision {
            action: Action::PickupLoot {
                // Reuse left-click action
                screen_x: pos.0 + jitter_x,
                screen_y: pos.1 + jitter_y,
            },
            delay: Duration::from_millis(self.rng.gen_range(150..400)),
            priority: 1,
            reason,
        }
    }

    // ─── Progression Integration ──────────────────────────────

    /// Access the progression engine (if enabled).
    pub fn progression(&self) -> Option<&ProgressionEngine> {
        self.progression.as_ref()
    }

    /// Mutable access to progression engine.
    pub fn progression_mut(&mut self) -> Option<&mut ProgressionEngine> {
        self.progression.as_mut()
    }

    /// Select the next script from the progression engine and load its plan
    /// into the ScriptExecutor.
    /// Returns true if a script was selected, false if all scripts are done
    /// (game should end).
    pub fn select_next_script(&mut self) -> bool {
        let progression = match self.progression.as_mut() {
            Some(p) => p,
            None => return false, // No progression = farming-only mode
        };

        match progression.next_script() {
            Some(script) => {
                let plan = script_plan(script, progression.state());
                tracing::info!("Selected script: {} ({} steps)", script.name(), plan.len());
                self.current_script = Some(script);
                self.script_steps = plan.clone();
                self.script_step_index = 0;
                self.step_started = Instant::now();
                // Load into executor
                self.executor.load_plan(plan);
                true
            }
            None => {
                tracing::info!("All scripts done for this game — exiting");
                self.current_script = None;
                self.script_steps.clear();
                self.executor.load_plan(Vec::new());
                false
            }
        }
    }

    /// Mark current script as complete in the progression engine.
    fn complete_current_script(&mut self) {
        if let Some(script) = self.current_script.take() {
            if let Some(ref mut progression) = self.progression {
                progression.mark_done(script);
            }
            tracing::info!("Script {} completed", script.name());
        }
    }

    /// Get the current script step (if executing a script).
    pub fn current_step(&self) -> Option<&ScriptStep> {
        if self.script_step_index < self.script_steps.len() {
            Some(&self.script_steps[self.script_step_index])
        } else {
            None
        }
    }

    /// Advance to the next script step.
    pub fn advance_step(&mut self) {
        self.script_step_index += 1;
        self.step_started = Instant::now();

        if self.script_step_index >= self.script_steps.len() {
            // Script complete
            if let Some(script) = self.current_script {
                if let Some(ref mut progression) = self.progression {
                    progression.mark_done(script);
                }
                tracing::info!("Script {} completed all steps", script.name());
            }
            self.current_script = None;
        }
    }

    /// Notify the progression engine that a quest was completed
    /// (called when vision detects a quest complete banner).
    pub fn on_quest_complete_visual(&mut self) {
        if let (Some(script), Some(ref mut progression)) =
            (self.current_script, self.progression.as_mut())
        {
            progression.on_quest_complete(script);
            tracing::info!("Quest completion detected for {}", script.name());
        }
    }

    /// Notify progression on game start.
    pub fn on_game_start_progression(&mut self) {
        if let Some(ref mut progression) = self.progression {
            progression.on_game_start();
        }
        self.current_script = None;
        self.script_steps.clear();
        self.script_step_index = 0;
        self.executor.load_plan(Vec::new());
    }

    /// Notify progression on game end + save state.
    pub fn on_game_end_progression(&mut self) {
        if let Some(ref mut progression) = self.progression {
            progression.on_game_end();
        }
    }

    /// Update character level from visual detection.
    pub fn update_level_from_vision(&mut self, level: u8) {
        if let Some(ref mut progression) = self.progression {
            progression.quest_state.update_level(level);
        }
    }

    /// Check if the current script step is a level gate that fails.
    /// If so, push retry and signal that the script should be abandoned.
    pub fn check_level_gate(&mut self, current_level: u8) -> bool {
        // Extract the min_level from current step without holding a borrow
        let min_level = if self.script_step_index < self.script_steps.len() {
            if let ScriptStep::RequireLevel { min_level } =
                self.script_steps[self.script_step_index]
            {
                Some(min_level)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(min_lvl) = min_level {
            if current_level < min_lvl {
                // Level too low — retry this script next game
                if let Some(script) = self.current_script {
                    tracing::info!(
                        "Level gate failed for {}: need {}, have {}",
                        script.name(),
                        min_lvl,
                        current_level
                    );
                    if let Some(ref mut progression) = self.progression {
                        progression.retry_next_game(script);
                    }
                }
                self.current_script = None;
                self.script_steps.clear();
                return true; // Gate failed
            }
        }
        false // Gate passed or no gate
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::FrameState;

    fn default_mgr() -> GameManager {
        GameManager::new(AgentConfig::default())
    }

    #[test]
    fn test_initial_phase_is_oog() {
        let mgr = default_mgr();
        assert_eq!(mgr.phase(), GamePhase::OutOfGame);
    }

    #[test]
    fn test_oog_to_town_transition() {
        let mut mgr = default_mgr();
        let mut state = FrameState::default();
        state.in_town = true;
        state.at_menu = false;
        state.loading_screen = false;

        // Requires 3 consecutive in_town frames for stable_in_town (in_town_frames >= 3)
        mgr.detect_phase_transitions(&state);
        mgr.detect_phase_transitions(&state);
        mgr.detect_phase_transitions(&state);
        assert_eq!(mgr.phase(), GamePhase::TownPrep);
        assert_eq!(mgr.game_count(), 1);
    }

    #[test]
    fn test_town_task_sequence() {
        let mut task = TownTask::Heal;
        let order = vec![
            TownTask::Heal,
            TownTask::Identify,
            TownTask::Stash,
            TownTask::BuyPotions,
            TownTask::Repair,
            TownTask::ReviveMerc,
            TownTask::Done,
        ];

        for expected in &order {
            assert_eq!(task, *expected);
            task = task.next();
        }
    }

    #[test]
    fn test_farming_delegates_to_engine() {
        let mut mgr = default_mgr();
        mgr.phase = GamePhase::Farming;

        let mut state = FrameState::default();
        state.in_town = false;
        state.at_menu = false;
        state.hp_pct = 90;
        state.mana_pct = 80;
        state.enemy_count = 3;
        state.in_combat = true;
        state.nearest_enemy_x = 640;
        state.nearest_enemy_y = 264;
        state.nearest_enemy_hp_pct = 100;

        let decision = mgr.decide(&state);
        // Should produce a combat action (attack, potion, etc.)
        assert!(
            !matches!(decision.action, Action::Wait),
            "farming phase should produce combat actions, got: {:?}",
            decision.action
        );
    }

    #[test]
    fn test_should_return_to_town_belt_low() {
        let mut mgr = default_mgr();
        mgr.phase = GamePhase::Farming;

        let mut state = FrameState::default();
        state.in_town = false;
        state.in_combat = false;
        state.enemy_count = 0;
        state.belt_columns = [0, 0, 0, 0]; // empty belt

        assert!(mgr.should_return_to_town(&state));
    }

    #[test]
    fn test_should_not_return_during_combat() {
        let mut mgr = default_mgr();
        mgr.phase = GamePhase::Farming;

        let mut state = FrameState::default();
        state.in_town = false;
        state.in_combat = true;
        state.enemy_count = 3;
        state.belt_columns = [0, 0, 0, 0]; // empty belt but in combat

        assert!(!mgr.should_return_to_town(&state));
    }

    #[test]
    fn test_chicken_transitions_to_exit() {
        let mut mgr = default_mgr();
        mgr.phase = GamePhase::Farming;

        let mut state = FrameState::default();
        state.in_town = false;
        state.at_menu = false;
        state.hp_pct = 20; // below chicken threshold (30)
        state.in_combat = true;
        state.enemy_count = 5;

        let decision = mgr.decide(&state);
        assert!(matches!(decision.action, Action::ChickenQuit));
        assert_eq!(mgr.phase(), GamePhase::ExitingGame);
        assert_eq!(mgr.total_chickens, 1);
    }

    #[test]
    fn test_inter_game_delay_completes() {
        let mut mgr = default_mgr();
        mgr.phase = GamePhase::InterGameDelay;
        mgr.last_game_exit = Instant::now() - Duration::from_secs(120); // well past delay
        mgr.config.farming.min_game_time_secs = 5;

        let decision = mgr.decide(&FrameState::default());
        assert_eq!(mgr.phase(), GamePhase::OutOfGame);
    }

    #[test]
    fn test_exit_sequence() {
        let mut mgr = default_mgr();
        mgr.phase = GamePhase::ExitingGame;

        let mut state = FrameState::default();
        state.at_menu = false;
        state.in_town = true;

        // Step 0: press escape
        let d1 = mgr.decide(&state);
        assert_eq!(mgr.exit_step, 1);

        // Step 1: click save & exit
        let d2 = mgr.decide(&state);
        assert_eq!(mgr.exit_step, 2);

        // Step 2: waiting
        let d3 = mgr.decide(&state);
        assert!(matches!(d3.action, Action::Wait));
    }

    #[test]
    fn test_npcs_all_acts() {
        for act in 1..=5 {
            let npcs = npcs_for_act(act);
            // All coordinates should be within reasonable screen bounds
            assert!(npcs.healer.0 > 0 && npcs.healer.0 < 800);
            assert!(npcs.stash.1 > 0 && npcs.stash.1 < 600);
            assert!(npcs._waypoint.0 > 0 && npcs._waypoint.0 < 800);
        }
    }
}
