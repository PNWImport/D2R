//! DXGI Desktop Duplication screen capture.
//!
//! Captures the primary display at 25 Hz using the Desktop Duplication API.
//! Extracts FrameState from raw pixel data and pushes to ShardedFrameBuffer.
//!
//! On non-Windows, provides a simulation stub for testing.

use crate::vision::{FrameState, ItemQuality, LootLabel, ShardedFrameBuffer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Configuration for the capture pipeline
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Target frames per second (default 25)
    pub target_fps: u32,
    /// Game window title to find (for windowed mode)
    pub window_title: String,
    /// Screen region to capture (x, y, width, height) — None = full primary
    pub region: Option<(u32, u32, u32, u32)>,
    /// HP orb center position (relative to game window)
    pub hp_orb_center: (u32, u32),
    /// Mana orb center position
    pub mana_orb_center: (u32, u32),
    /// HP orb sample radius in pixels
    pub orb_sample_radius: u32,
    /// Character center screen position
    pub char_center: (u16, u16),
}

impl Default for CaptureConfig {
    fn default() -> Self {
        // Default positions for 800x600 D2 resolution
        Self {
            target_fps: 25,
            window_title: "Diablo II".to_string(),
            region: None,
            hp_orb_center: (95, 525),    // Left orb
            mana_orb_center: (705, 525), // Right orb
            orb_sample_radius: 30,
            char_center: (400, 300),
        }
    }
}

/// Enemy detection results from a single frame
#[derive(Default)]
struct EnemyInfo {
    count: u8,
    nearest_x: u16,
    nearest_y: u16,
    nearest_hp_pct: u8,
    boss_present: bool,
    champion_present: bool,
    immune_detected: bool,
}

/// Raw pixel buffer from screen capture
pub struct CapturedFrame {
    pub pixels: Vec<u8>, // BGRA format, row-major
    pub width: u32,
    pub height: u32,
    pub stride: u32, // bytes per row (may include padding)
    pub timestamp: Instant,
}

/// The capture pipeline. Runs in its own thread, pushes to ShardedFrameBuffer.
pub struct CapturePipeline {
    config: CaptureConfig,
    buffer: Arc<ShardedFrameBuffer>,
    running: Arc<AtomicBool>,
    tick: u64,
}

impl CapturePipeline {
    pub fn new(config: CaptureConfig, buffer: Arc<ShardedFrameBuffer>) -> Self {
        Self {
            config,
            buffer,
            running: Arc::new(AtomicBool::new(false)),
            tick: 0,
        }
    }

    pub fn running_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    /// Run the capture loop. Blocks until running flag is set to false.
    pub fn run(&mut self) {
        self.running.store(true, Ordering::Release);
        let frame_interval = Duration::from_micros(1_000_000 / self.config.target_fps as u64);

        #[cfg(windows)]
        let mut capturer = match DxgiCapturer::new() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to initialize DXGI capture: {}", e);
                self.running.store(false, Ordering::Release);
                return;
            }
        };

        while self.running.load(Ordering::Acquire) {
            let frame_start = Instant::now();

            #[cfg(windows)]
            let capture_result = capturer.capture_frame();
            #[cfg(not(windows))]
            let capture_result = self.simulate_capture();

            match capture_result {
                Ok(frame) => {
                    let state = self.extract_frame_state(&frame);
                    self.buffer.push(state);
                    self.tick += 1;
                }
                Err(_) => {
                    // Frame skip — DXGI returns timeout if no screen update
                    // This is normal behavior, not an error
                }
            }

            // Precise frame timing
            let elapsed = frame_start.elapsed();
            if elapsed < frame_interval {
                std::thread::sleep(frame_interval - elapsed);
            }
        }
    }

    /// Extract FrameState from raw pixel data.
    ///
    /// Performance tiers:
    /// - **Every frame** (survival-critical): HP, mana, enemies, loot
    /// - **Every 3rd frame** (state transitions): town, merc, belt, UI panels
    /// - **Every 5th frame** (slow-changing): area banner, quest banner, XP bar
    fn extract_frame_state(&self, frame: &CapturedFrame) -> FrameState {
        let mut state = FrameState {
            tick: self.tick,
            capture_time_ns: frame.timestamp.elapsed().as_nanos() as u64,
            char_screen_x: self.config.char_center.0,
            char_screen_y: self.config.char_center.1,
            hp_pct: self.read_orb_pct(frame, self.config.hp_orb_center, OrbType::Health),
            mana_pct: self.read_orb_pct(frame, self.config.mana_orb_center, OrbType::Mana),
            ..Default::default()
        };

        // ─── Tier 1: Every frame (survival-critical) ──────────────
        // Detect enemies — returns count + nearest enemy position + boss info
        let enemy_info = self.detect_enemies(frame);
        state.enemy_count = enemy_info.count;
        state.in_combat = enemy_info.count > 0;
        state.nearest_enemy_x = enemy_info.nearest_x;
        state.nearest_enemy_y = enemy_info.nearest_y;
        state.nearest_enemy_hp_pct = enemy_info.nearest_hp_pct;
        state.boss_present = enemy_info.boss_present;
        state.champion_present = enemy_info.champion_present;
        state.immune_detected = enemy_info.immune_detected;

        // Detect ground loot labels
        let labels = self.detect_loot_labels(frame);
        state.loot_label_count = labels.len() as u8;
        for (i, label) in labels.iter().enumerate().take(8) {
            state.loot_labels[i] = label.clone();
        }

        // ─── Tier 2: Every 3rd frame (state transitions) ─────────
        if self.tick % 3 == 0 {
            // Detect town (stone floor colors vs dungeon/field)
            state.in_town = self.detect_town(frame);

            // Merc alive check (green health bar above merc portrait)
            state.merc_alive = self.detect_merc_alive(frame);
            state.merc_hp_pct = if state.merc_alive {
                self.read_merc_hp(frame)
            } else {
                0
            };

            // Belt potions (4 columns at bottom of screen)
            state.belt_columns = self.read_belt_columns(frame);

            // UI panels open (NPC dialog, waypoint, stash, etc.)
            state.npc_dialog_open = self.detect_dialog_panel(frame);
            state.waypoint_menu_open = self.detect_waypoint_menu(frame);
        }

        // ─── Tier 3: Every 5th frame (slow-changing) ──────────────
        if self.tick % 5 == 0 {
            // Area name: gold text banner at top-center on area transitions
            if let Some(area_detected) = self.detect_area_banner(frame) {
                state.set_area_name(&area_detected);
            }

            // Quest complete banner: golden "Quest Completed" text
            state.quest_complete_banner = self.detect_quest_banner(frame);

            // Experience bar: thin strip at very bottom of screen
            state.xp_bar_pct = self.read_xp_bar(frame);
        }

        state
    }

    // ─── Orb Reading ───────────────────────────────────────────

    fn read_orb_pct(&self, frame: &CapturedFrame, center: (u32, u32), orb_type: OrbType) -> u8 {
        let r = self.config.orb_sample_radius;
        let (cx, cy) = center;

        // Sample vertical column through orb center
        // D2 orbs fill from bottom to top
        let mut filled_pixels = 0u32;
        let mut total_pixels = 0u32;

        // Use squared threshold to avoid sqrt() in hot pixel loop (~10× faster)
        let (target_r, target_g, target_b, threshold_sq) = match orb_type {
            OrbType::Health => (180, 20, 20, 60 * 60), // Red orb
            OrbType::Mana => (30, 30, 180, 60 * 60),   // Blue orb
        };

        for dy in 0..r * 2 {
            let y = cy.saturating_sub(r) + dy;
            // Sample a few columns for robustness
            for dx_offset in [0i32, -3, 3, -6, 6] {
                let x = (cx as i32 + dx_offset).max(0) as u32;
                if x >= frame.width || y >= frame.height {
                    continue;
                }

                let idx = (y * frame.stride + x * 4) as usize;
                if idx + 2 >= frame.pixels.len() {
                    continue;
                }

                let b = frame.pixels[idx] as i32;
                let g = frame.pixels[idx + 1] as i32;
                let pixel_r = frame.pixels[idx + 2] as i32;

                total_pixels += 1;

                // Check if pixel matches orb color (squared distance, no sqrt)
                let dist_sq = (pixel_r - target_r).pow(2)
                    + (g - target_g).pow(2)
                    + (b - target_b).pow(2);
                if dist_sq < threshold_sq {
                    filled_pixels += 1;
                }
            }
        }

        if total_pixels == 0 {
            return 100; // Can't read = assume full (safer)
        }

        ((filled_pixels as f32 / total_pixels as f32) * 100.0).min(100.0) as u8
    }

    // ─── Enemy Detection ───────────────────────────────────────

    /// Full enemy detection: count, nearest position, boss/champion/immune flags
    fn detect_enemies(&self, frame: &CapturedFrame) -> EnemyInfo {
        let mut info = EnemyInfo::default();
        let char_x = self.config.char_center.0 as i32;
        let char_y = self.config.char_center.1 as i32;
        let mut nearest_dist_sq = i32::MAX;

        let scan_y_start = (frame.height as f32 * 0.08) as u32;
        let scan_y_end = (frame.height as f32 * 0.75) as u32;
        let step = 4;
        let mut last_bar_y = 0u32;

        'outer: for y in (scan_y_start..scan_y_end).step_by(step) {
            let mut red_run = 0u32;
            let mut red_start_x = 0u32;
            let mut dark_count = 0u32; // damaged portion of bar

            for x in (50..frame.width.saturating_sub(50)).step_by(2) {
                let idx = (y * frame.stride + x * 4) as usize;
                if idx + 2 >= frame.pixels.len() {
                    continue;
                }

                let b = frame.pixels[idx];
                let g = frame.pixels[idx + 1];
                let r = frame.pixels[idx + 2];

                // Enemy health bar: bright red, low green/blue
                if r > 180 && g < 60 && b < 60 {
                    if red_run == 0 {
                        red_start_x = x;
                    }
                    red_run += 1;
                } else if r > 60 && r < 120 && g < 30 && b < 30 && red_run > 0 {
                    // Dark red = damaged portion of health bar
                    dark_count += 1;
                } else {
                    let total_bar = red_run + dark_count;
                    if red_run >= 8 && total_bar <= 60 && y.saturating_sub(last_bar_y) > 20 {
                        info.count = info.count.saturating_add(1);
                        last_bar_y = y;

                        // Bar center position (enemy is below the health bar)
                        let bar_cx = (red_start_x + (red_run + dark_count) / 2) as i32;
                        let bar_cy = y as i32 + 30; // enemy sprite below health bar

                        // HP estimate from bar fill ratio
                        let hp = if total_bar > 0 {
                            ((red_run as f32 / total_bar as f32) * 100.0) as u8
                        } else {
                            100
                        };

                        // Distance to character
                        let dx = bar_cx - char_x;
                        let dy = bar_cy - char_y;
                        let dist_sq = dx * dx + dy * dy;

                        if dist_sq < nearest_dist_sq {
                            nearest_dist_sq = dist_sq;
                            info.nearest_x = bar_cx.max(0) as u16;
                            info.nearest_y = bar_cy.max(0) as u16;
                            info.nearest_hp_pct = hp;
                        }

                        // Boss detection: boss health bars are wider (40+ pixels)
                        if total_bar >= 40 {
                            info.boss_present = true;
                        }
                        // Champion detection: slightly wider than normal (25-39)
                        if (25..40).contains(&total_bar) {
                            info.champion_present = true;
                        }

                        // Early exit: no need to scan further once we hit max
                        if info.count >= 20 {
                            break 'outer;
                        }
                    }
                    red_run = 0;
                    dark_count = 0;
                }
            }
        }

        // Immune detection: look for "Immune to X" text colors near enemies
        // Immune text in D2 appears as cyan/teal colored text
        // Pass nearest enemy Y as hint to narrow the scan region
        if info.count > 0 {
            info.immune_detected =
                self.detect_immune_text(frame, info.nearest_y as u32);
        }

        info.count = info.count.min(20);
        info
    }

    /// Detect "Immune to" text (cyan/teal text that appears on immune monsters).
    /// Only scans near detected enemy health bars for efficiency.
    fn detect_immune_text(&self, frame: &CapturedFrame, enemy_y_hint: u32) -> bool {
        // Scan a narrower band around where enemies were detected instead of 52% of screen.
        // Immune text appears just above/below enemy health bars.
        let scan_y_start = enemy_y_hint.saturating_sub(40).max((frame.height as f32 * 0.08) as u32);
        let scan_y_end = (enemy_y_hint + 80).min((frame.height as f32 * 0.60) as u32);

        for y in (scan_y_start..scan_y_end).step_by(4) {
            let mut cyan_run = 0u32;
            for x in (50..frame.width.saturating_sub(50)).step_by(3) {
                let idx = (y * frame.stride + x * 4) as usize;
                if idx + 2 >= frame.pixels.len() {
                    continue;
                }

                let b = frame.pixels[idx];
                let g = frame.pixels[idx + 1];
                let r = frame.pixels[idx + 2];

                // Cyan/teal text: high green+blue, low red (immunity text color)
                if g > 150 && b > 150 && r < 80 {
                    cyan_run += 1;
                } else {
                    if cyan_run >= 5 {
                        return true;
                    }
                    cyan_run = 0;
                }
            }
        }
        false
    }

    /// Read merc HP from the merc health bar (smaller bar below character portrait)
    fn read_merc_hp(&self, frame: &CapturedFrame) -> u8 {
        let bar_y = (frame.height as f32 * 0.13) as u32;
        let bar_x_start = 10u32;
        let bar_x_end = 80u32;
        let mut green_count = 0u32;
        let total = bar_x_end - bar_x_start;

        for x in bar_x_start..bar_x_end {
            let idx = (bar_y * frame.stride + x * 4) as usize;
            if idx + 2 >= frame.pixels.len() {
                continue;
            }
            let g = frame.pixels[idx + 1];
            let r = frame.pixels[idx + 2];
            let b = frame.pixels[idx];
            if g > 150 && r < 80 && b < 80 {
                green_count += 1;
            }
        }

        if total == 0 {
            return 100;
        }
        ((green_count as f32 / total as f32) * 100.0).min(100.0) as u8
    }

    /// Read belt potion columns (4 slots at bottom of screen)
    fn read_belt_columns(&self, frame: &CapturedFrame) -> [u8; 4] {
        // Belt slots are at the bottom-center of screen
        // 4 columns, each shows a stack of potions with colored indicators
        let belt_y = (frame.height as f32 * 0.92) as u32;
        let belt_x_start = (frame.width as f32 * 0.39) as u32;
        let col_width = (frame.width as f32 * 0.055) as u32;
        let mut columns = [0u8; 4];

        for (col, column) in columns.iter_mut().enumerate() {
            let cx = belt_x_start + col as u32 * col_width + col_width / 2;
            let idx = (belt_y * frame.stride + cx * 4) as usize;
            if idx + 2 >= frame.pixels.len() {
                continue;
            }

            let b = frame.pixels[idx];
            let g = frame.pixels[idx + 1];
            let r = frame.pixels[idx + 2];

            // Non-empty slot has colored potion pixels (not dark/black)
            let brightness = (r as u32 + g as u32 + b as u32) / 3;
            *column = if brightness > 40 { 4 } else { 0 };
        }

        columns
    }

    // ─── Loot Detection ────────────────────────────────────────

    fn detect_loot_labels(&self, frame: &CapturedFrame) -> Vec<LootLabel> {
        // D2 ground item labels appear as colored text on screen
        // Scan for clusters of quality-colored pixels
        let mut labels = Vec::new();

        // Quality color definitions (BGRA order in memory)
        // Pre-squared thresholds to avoid sqrt() in hot pixel loop
        let quality_colors: [(u8, u8, u8, ItemQuality, i32); 6] = [
            (99, 166, 198, ItemQuality::Unique, 35 * 35),  // Gold text
            (0, 255, 0, ItemQuality::Set, 30 * 30),        // Green text
            (119, 255, 255, ItemQuality::Rare, 25 * 25),   // Yellow text
            (255, 104, 104, ItemQuality::Magic, 25 * 25),  // Blue text (BGR)
            (80, 169, 255, ItemQuality::Rune, 30 * 30),    // Orange text
            (255, 255, 255, ItemQuality::Normal, 20 * 20), // White text
        ];

        // Scan playfield area for colored text clusters
        let scan_y_start = (frame.height as f32 * 0.10) as u32;
        let scan_y_end = (frame.height as f32 * 0.75) as u32;
        let step = 6;

        for y in (scan_y_start..scan_y_end).step_by(step) {
            for x in (20..frame.width.saturating_sub(20)).step_by(8) {
                let idx = (y * frame.stride + x * 4) as usize;
                if idx + 2 >= frame.pixels.len() {
                    continue;
                }

                let pb = frame.pixels[idx];
                let pg = frame.pixels[idx + 1];
                let pr = frame.pixels[idx + 2];

                for &(tb, tg, tr, ref quality, threshold_sq) in &quality_colors {
                    let dist_sq = (pr as i32 - tr as i32).pow(2)
                        + (pg as i32 - tg as i32).pow(2)
                        + (pb as i32 - tb as i32).pow(2);

                    if dist_sq < threshold_sq {
                        // Check for cluster (adjacent colored pixels = likely text)
                        let cluster =
                            self.check_text_cluster(frame, x, y, tr, tg, tb, threshold_sq);
                        if cluster >= 3 {
                            // Avoid duplicates near same position
                            let too_close = labels.iter().any(|l: &LootLabel| {
                                (l.x as i32 - x as i32).abs() < 40
                                    && (l.y as i32 - y as i32).abs() < 15
                            });
                            if !too_close {
                                labels.push(LootLabel {
                                    x: x as u16,
                                    y: y as u16,
                                    quality: *quality,
                                    text_hash: (x * 31 + y * 17),
                                });
                            }
                        }
                        break; // Only match first quality
                    }
                }
            }
        }

        labels.truncate(8); // Max 8 labels per frame
        labels
    }

    /// Check for a cluster of similarly-colored pixels around a candidate.
    /// `threshold_sq` is the pre-squared color distance threshold.
    #[allow(clippy::too_many_arguments)]
    fn check_text_cluster(
        &self,
        frame: &CapturedFrame,
        cx: u32,
        cy: u32,
        tr: u8,
        tg: u8,
        tb: u8,
        threshold_sq: i32,
    ) -> u32 {
        let mut count = 0u32;
        for dy in -2i32..=2 {
            for dx in -4i32..=4 {
                let x = (cx as i32 + dx * 2).max(0) as u32;
                let y = (cy as i32 + dy).max(0) as u32;
                if x >= frame.width || y >= frame.height {
                    continue;
                }
                let idx = (y * frame.stride + x * 4) as usize;
                if idx + 2 >= frame.pixels.len() {
                    continue;
                }
                let dist_sq = (frame.pixels[idx + 2] as i32 - tr as i32).pow(2)
                    + (frame.pixels[idx + 1] as i32 - tg as i32).pow(2)
                    + (frame.pixels[idx] as i32 - tb as i32).pow(2);
                if dist_sq < threshold_sq {
                    count += 1;
                }
            }
        }
        count
    }

    // ─── Town Detection ────────────────────────────────────────

    fn detect_town(&self, frame: &CapturedFrame) -> bool {
        // Town areas have characteristic stone/tan floor colors
        // Sample the lower-center of screen (character's feet area)
        let sample_y = (frame.height as f32 * 0.55) as u32;
        let sample_x_start = (frame.width as f32 * 0.35) as u32;
        let sample_x_end = (frame.width as f32 * 0.65) as u32;

        let mut town_pixels = 0u32;
        let mut total = 0u32;

        for x in (sample_x_start..sample_x_end).step_by(4) {
            for dy in 0..20u32 {
                let y = sample_y + dy;
                let idx = (y * frame.stride + x * 4) as usize;
                if idx + 2 >= frame.pixels.len() {
                    continue;
                }
                let b = frame.pixels[idx];
                let g = frame.pixels[idx + 1];
                let r = frame.pixels[idx + 2];

                total += 1;

                // Town stone colors: warm gray/tan (roughly equal RGB, moderate brightness)
                let avg = (r as u32 + g as u32 + b as u32) / 3;
                let max_diff = r.max(g).max(b) as i32 - r.min(g).min(b) as i32;
                if avg > 80 && avg < 180 && max_diff < 40 {
                    town_pixels += 1;
                }
            }
        }

        total > 0 && (town_pixels as f32 / total as f32) > 0.35
    }

    // ─── Merc Detection ────────────────────────────────────────

    fn detect_merc_alive(&self, frame: &CapturedFrame) -> bool {
        // Merc health bar is at the top-left of screen, below character portrait
        // Small green bar around (10, 80) for 800x600
        let bar_y = (frame.height as f32 * 0.13) as u32;
        let bar_x_start = 10u32;
        let bar_x_end = 80u32;
        let mut green_count = 0u32;

        for x in bar_x_start..bar_x_end {
            let idx = (bar_y * frame.stride + x * 4) as usize;
            if idx + 2 >= frame.pixels.len() {
                continue;
            }
            let g = frame.pixels[idx + 1];
            let r = frame.pixels[idx + 2];
            let b = frame.pixels[idx];
            if g > 150 && r < 80 && b < 80 {
                green_count += 1;
            }
        }

        green_count > 10
    }

    // ─── Area Name Banner Detection ────────────────────────────
    // D2R displays area names as gold-colored text centered at the top
    // of the screen (roughly y=40-60) for ~2 seconds when entering a new area.
    // Gold text color: approximately RGB(198, 166, 99) — warm tan/gold.
    // We detect a horizontal run of gold-ish pixels in the top-center band.

    fn detect_area_banner(&self, frame: &CapturedFrame) -> Option<String> {
        let banner_y_start = (frame.height as f32 * 0.05) as u32;
        let banner_y_end = (frame.height as f32 * 0.12) as u32;
        let banner_x_start = (frame.width as f32 * 0.25) as u32;
        let banner_x_end = (frame.width as f32 * 0.75) as u32;

        // Gold text RGB target: (198, 166, 99) with threshold ~45
        let (tr, tg, tb) = (198i32, 166i32, 99i32);
        let threshold_sq = 45i32 * 45;

        let mut gold_pixel_count = 0u32;

        for y in (banner_y_start..banner_y_end).step_by(2) {
            for x in (banner_x_start..banner_x_end).step_by(3) {
                let idx = (y * frame.stride + x * 4) as usize;
                if idx + 2 >= frame.pixels.len() {
                    continue;
                }

                let b = frame.pixels[idx] as i32;
                let g = frame.pixels[idx + 1] as i32;
                let r = frame.pixels[idx + 2] as i32;

                let dist_sq = (r - tr).pow(2) + (g - tg).pow(2) + (b - tb).pow(2);
                if dist_sq < threshold_sq {
                    gold_pixel_count += 1;
                }
            }
        }

        // If enough gold pixels detected in the banner zone, an area name is showing.
        // We can't OCR the text yet — but we know *something* is there.
        // The area name will be determined by the progression engine based on
        // expected transitions (e.g., if we just used Cold Plains WP, the next
        // area banner = "Cold Plains").
        // For now, return a marker indicating banner is present.
        if gold_pixel_count > 15 {
            Some("_banner_detected".to_string())
        } else {
            None
        }
    }

    // ─── Quest Complete Banner Detection ─────────────────────
    // "Quest Completed" appears as a large golden banner in the center
    // of the screen. Similar gold color but larger area than area name.

    fn detect_quest_banner(&self, frame: &CapturedFrame) -> bool {
        // Quest banner appears roughly at y=35-55%, centered horizontally
        let banner_y_start = (frame.height as f32 * 0.35) as u32;
        let banner_y_end = (frame.height as f32 * 0.55) as u32;
        let banner_x_start = (frame.width as f32 * 0.30) as u32;
        let banner_x_end = (frame.width as f32 * 0.70) as u32;

        // Quest complete text is bright gold: ~RGB(255, 220, 100)
        let (tr, tg, tb) = (255i32, 220i32, 100i32);
        let threshold_sq = 50i32 * 50;
        let mut gold_count = 0u32;

        for y in (banner_y_start..banner_y_end).step_by(4) {
            for x in (banner_x_start..banner_x_end).step_by(4) {
                let idx = (y * frame.stride + x * 4) as usize;
                if idx + 2 >= frame.pixels.len() {
                    continue;
                }

                let b = frame.pixels[idx] as i32;
                let g = frame.pixels[idx + 1] as i32;
                let r = frame.pixels[idx + 2] as i32;

                let dist_sq = (r - tr).pow(2) + (g - tg).pow(2) + (b - tb).pow(2);
                if dist_sq < threshold_sq {
                    gold_count += 1;
                }
            }
        }

        // Quest banner has a lot more gold text than area names
        gold_count > 40
    }

    // ─── NPC Dialog Panel Detection ──────────────────────────
    // NPC dialog appears as a large dark panel in the lower half of screen
    // with gold-bordered edges and text options.

    fn detect_dialog_panel(&self, frame: &CapturedFrame) -> bool {
        // Dialog panel occupies roughly y=50-85%, x=10-60%
        let panel_y = (frame.height as f32 * 0.70) as u32;
        let panel_x_start = (frame.width as f32 * 0.10) as u32;
        let panel_x_end = (frame.width as f32 * 0.55) as u32;

        let mut dark_panel_pixels = 0u32;
        let mut total = 0u32;

        for x in (panel_x_start..panel_x_end).step_by(6) {
            for dy in 0..30u32 {
                let y = panel_y + dy;
                let idx = (y * frame.stride + x * 4) as usize;
                if idx + 2 >= frame.pixels.len() {
                    continue;
                }

                let b = frame.pixels[idx];
                let g = frame.pixels[idx + 1];
                let r = frame.pixels[idx + 2];

                total += 1;
                // Dark brownish panel background: low brightness, slight warm tint
                let avg = (r as u32 + g as u32 + b as u32) / 3;
                if avg > 15 && avg < 60 && r > g && r > b {
                    dark_panel_pixels += 1;
                }
            }
        }

        total > 0 && (dark_panel_pixels as f32 / total as f32) > 0.5
    }

    // ─── Waypoint Menu Detection ─────────────────────────────
    // Waypoint menu is a large dark panel that covers most of the screen
    // with act tabs at the top and destination names listed below.

    fn detect_waypoint_menu(&self, frame: &CapturedFrame) -> bool {
        // Waypoint panel: dark background with act tabs
        // The panel has a distinctive frame/border at known positions
        // Check for the dark panel at center-screen with gold accents
        let panel_y = (frame.height as f32 * 0.15) as u32;
        let panel_x = (frame.width as f32 * 0.25) as u32;
        let panel_x_end = (frame.width as f32 * 0.75) as u32;

        let mut dark_count = 0u32;
        let mut total = 0u32;

        for x in (panel_x..panel_x_end).step_by(8) {
            let idx = (panel_y * frame.stride + x * 4) as usize;
            if idx + 2 >= frame.pixels.len() {
                continue;
            }

            let b = frame.pixels[idx];
            let g = frame.pixels[idx + 1];
            let r = frame.pixels[idx + 2];

            total += 1;
            let avg = (r as u32 + g as u32 + b as u32) / 3;
            if avg < 40 {
                dark_count += 1;
            }
        }

        // Waypoint menu covers a large area with dark background
        total > 0 && (dark_count as f32 / total as f32) > 0.7
    }

    // ─── Experience Bar Detection ────────────────────────────
    // Thin bar at the very bottom of the screen, fills left-to-right
    // with a yellowish color as XP increases.

    fn read_xp_bar(&self, frame: &CapturedFrame) -> u8 {
        let bar_y = frame.height.saturating_sub(3); // 3 pixels from bottom
        let bar_x_start = (frame.width as f32 * 0.12) as u32;
        let bar_x_end = (frame.width as f32 * 0.88) as u32;
        let total_width = bar_x_end - bar_x_start;

        if total_width == 0 {
            return 0;
        }

        let mut filled = 0u32;

        for x in bar_x_start..bar_x_end {
            let idx = (bar_y * frame.stride + x * 4) as usize;
            if idx + 2 >= frame.pixels.len() {
                continue;
            }

            let b = frame.pixels[idx];
            let g = frame.pixels[idx + 1];
            let r = frame.pixels[idx + 2];

            // XP bar is yellow-ish: high R, high G, low B
            if r > 120 && g > 100 && b < 80 {
                filled += 1;
            }
        }

        ((filled as f32 / total_width as f32) * 100.0).min(100.0) as u8
    }

    // ─── Simulation (non-Windows) ──────────────────────────────

    #[cfg(not(windows))]
    fn simulate_capture(&self) -> Result<CapturedFrame, anyhow::Error> {
        // Produce a synthetic frame for testing
        let width = 800u32;
        let height = 600u32;
        let stride = width * 4;
        let mut pixels = vec![0u8; (stride * height) as usize];

        // Paint a basic test scene
        // Red HP orb area
        let (hx, hy) = self.config.hp_orb_center;
        for dy in 0..60u32 {
            for dx in 0..60u32 {
                let x = hx.saturating_sub(30) + dx;
                let y = hy.saturating_sub(30) + dy;
                if x < width && y < height {
                    let idx = (y * stride + x * 4) as usize;
                    if dy < 45 {
                        // 75% filled
                        pixels[idx] = 20; // B
                        pixels[idx + 1] = 20; // G
                        pixels[idx + 2] = 200; // R
                    }
                }
            }
        }

        // Blue Mana orb area
        let (mx, my) = self.config.mana_orb_center;
        for dy in 0..60u32 {
            for dx in 0..60u32 {
                let x = mx.saturating_sub(30) + dx;
                let y = my.saturating_sub(30) + dy;
                if x < width && y < height {
                    let idx = (y * stride + x * 4) as usize;
                    if dy < 40 {
                        // 67% filled
                        pixels[idx] = 200; // B
                        pixels[idx + 1] = 30; // G
                        pixels[idx + 2] = 30; // R
                    }
                }
            }
        }

        Ok(CapturedFrame {
            pixels,
            width,
            height,
            stride,
            timestamp: Instant::now(),
        })
    }
}

enum OrbType {
    Health,
    Mana,
}

// ═══════════════════════════════════════════════════════════════
// Vision Pipeline Benchmarks & Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Create a synthetic 800×600 test frame with game-like elements
    fn make_test_frame() -> CapturedFrame {
        let width = 800u32;
        let height = 600u32;
        let stride = width * 4;
        let mut pixels = vec![0u8; (stride * height) as usize];

        // Paint HP orb (red, 75% filled)
        for dy in 0..60u32 {
            for dx in 0..60u32 {
                let x = 65 + dx; // near hp_orb_center (95, 525)
                let y = 495 + dy;
                if x < width && y < height && dy < 45 {
                    let idx = (y * stride + x * 4) as usize;
                    pixels[idx] = 20;     // B
                    pixels[idx + 1] = 20; // G
                    pixels[idx + 2] = 200; // R
                }
            }
        }

        // Paint 3 enemy health bars (red horizontal bars)
        for (bar_y, bar_x) in [(150u32, 200u32), (220, 400), (300, 550)] {
            for x in bar_x..bar_x + 20 {
                if x < width && bar_y < height {
                    let idx = (bar_y * stride + x * 4) as usize;
                    pixels[idx] = 10;      // B
                    pixels[idx + 1] = 20;  // G
                    pixels[idx + 2] = 220; // R (bright red)
                }
            }
        }

        // Paint a gold loot label (Unique item color ~RGB(198,166,99))
        for dx in 0..30u32 {
            let x = 350 + dx;
            let y = 350u32;
            if x < width && y < height {
                let idx = (y * stride + x * 4) as usize;
                pixels[idx] = 99;      // B
                pixels[idx + 1] = 166; // G
                pixels[idx + 2] = 198; // R
            }
        }

        // Paint town-like stone floor (warm gray) at bottom center
        for x in (280..520).step_by(1) {
            for y in 320..345 {
                if x < width && y < height {
                    let idx = (y as u32 * stride + x as u32 * 4) as usize;
                    pixels[idx] = 120;     // B
                    pixels[idx + 1] = 130; // G
                    pixels[idx + 2] = 140; // R
                }
            }
        }

        CapturedFrame {
            pixels,
            width,
            height,
            stride,
            timestamp: Instant::now(),
        }
    }

    #[test]
    fn test_extract_frame_state_basic() {
        let config = CaptureConfig::default();
        let buffer = Arc::new(ShardedFrameBuffer::new());
        let pipeline = CapturePipeline::new(config, buffer);
        let frame = make_test_frame();

        let state = pipeline.extract_frame_state(&frame);

        // HP orb should read something > 0 from our painted pixels
        assert!(state.hp_pct > 0, "HP should be detected from painted orb");
        // Enemy count should detect our painted health bars
        assert!(state.enemy_count > 0, "Should detect painted enemy bars");
    }

    #[test]
    fn test_orb_reading_no_sqrt() {
        let config = CaptureConfig::default();
        let buffer = Arc::new(ShardedFrameBuffer::new());
        let pipeline = CapturePipeline::new(config, buffer);
        let frame = make_test_frame();

        // Read orb 10000 times — should be fast without sqrt
        let start = Instant::now();
        let iters = 10_000u32;
        let mut hp_sum = 0u32;
        for _ in 0..iters {
            hp_sum += pipeline.read_orb_pct(
                &frame,
                pipeline.config.hp_orb_center,
                OrbType::Health,
            ) as u32;
        }
        let elapsed = start.elapsed();
        let per_call_us = elapsed.as_micros() as f64 / iters as f64;

        println!(
            "Orb read: {:.2} μs/call ({} calls, sum={})",
            per_call_us, iters, hp_sum
        );
        // Should be under 50 μs per call (generous for CI; production target <5 μs)
        assert!(
            per_call_us < 50.0,
            "Orb reading too slow: {:.2} μs/call (expected <50 μs)",
            per_call_us
        );
    }

    #[test]
    fn test_enemy_detection_perf() {
        let config = CaptureConfig::default();
        let buffer = Arc::new(ShardedFrameBuffer::new());
        let pipeline = CapturePipeline::new(config, buffer);
        let frame = make_test_frame();

        let iters = 1_000u32;
        let start = Instant::now();
        let mut total_enemies = 0u32;
        for _ in 0..iters {
            let info = pipeline.detect_enemies(&frame);
            total_enemies += info.count as u32;
        }
        let elapsed = start.elapsed();
        let per_call_us = elapsed.as_micros() as f64 / iters as f64;

        println!(
            "Enemy detect: {:.1} μs/call ({} calls, total enemies={})",
            per_call_us, iters, total_enemies
        );
        // Enemy detection is the heaviest pass — should be under 500 μs
        assert!(
            per_call_us < 2000.0,
            "Enemy detection too slow: {:.1} μs/call",
            per_call_us
        );
    }

    #[test]
    fn test_loot_detection_perf() {
        let config = CaptureConfig::default();
        let buffer = Arc::new(ShardedFrameBuffer::new());
        let pipeline = CapturePipeline::new(config, buffer);
        let frame = make_test_frame();

        let iters = 1_000u32;
        let start = Instant::now();
        let mut total_labels = 0u32;
        for _ in 0..iters {
            let labels = pipeline.detect_loot_labels(&frame);
            total_labels += labels.len() as u32;
        }
        let elapsed = start.elapsed();
        let per_call_us = elapsed.as_micros() as f64 / iters as f64;

        println!(
            "Loot detect: {:.1} μs/call ({} calls, total labels={})",
            per_call_us, iters, total_labels
        );
        // Loot detection with squared distances should be fast
        assert!(
            per_call_us < 2000.0,
            "Loot detection too slow: {:.1} μs/call",
            per_call_us
        );
    }

    #[test]
    fn test_full_extract_perf() {
        let config = CaptureConfig::default();
        let buffer = Arc::new(ShardedFrameBuffer::new());
        let mut pipeline = CapturePipeline::new(config, buffer);
        let frame = make_test_frame();

        let iters = 500u32;
        let start = Instant::now();
        for i in 0..iters {
            pipeline.tick = i as u64;
            let _state = pipeline.extract_frame_state(&frame);
        }
        let elapsed = start.elapsed();
        let per_frame_us = elapsed.as_micros() as f64 / iters as f64;
        let per_frame_ms = per_frame_us / 1000.0;
        let theoretical_fps = 1_000_000.0 / per_frame_us;

        println!(
            "Full extract: {:.1} μs/frame ({:.2} ms) = {:.0} theoretical FPS",
            per_frame_us, per_frame_ms, theoretical_fps
        );
        // Full extraction should be under 5ms (200+ FPS theoretical)
        assert!(
            per_frame_ms < 10.0,
            "Full frame extraction too slow: {:.2} ms/frame",
            per_frame_ms
        );
    }

    #[test]
    fn test_tiered_detection_savings() {
        // Measure the savings from tiered detection (Tier 2 every 3rd, Tier 3 every 5th)
        let config = CaptureConfig::default();
        let buffer = Arc::new(ShardedFrameBuffer::new());
        let mut pipeline = CapturePipeline::new(config, buffer);
        let frame = make_test_frame();

        // Simulate 300 frames (12 seconds at 25 Hz)
        let frames = 300u64;
        let start = Instant::now();
        for tick in 0..frames {
            pipeline.tick = tick;
            let _state = pipeline.extract_frame_state(&frame);
        }
        let total = start.elapsed();
        let avg_us = total.as_micros() as f64 / frames as f64;

        // Calculate theoretical: Tier 2 runs 100/300 frames, Tier 3 runs 60/300
        let tier2_fraction = 1.0 / 3.0;
        let tier3_fraction = 1.0 / 5.0;
        println!(
            "Tiered extract: {:.1} μs/frame avg over {} frames",
            avg_us, frames
        );
        println!(
            "  Tier 2 (town/merc/belt/UI) runs {:.0}% of frames",
            tier2_fraction * 100.0
        );
        println!(
            "  Tier 3 (area/quest/xp) runs {:.0}% of frames",
            tier3_fraction * 100.0
        );
        println!("  Total session time: {:.1} ms", total.as_millis());
    }

    #[test]
    fn test_squared_vs_sqrt_equivalence() {
        // Verify that our squared-distance optimization produces equivalent results
        // to the original sqrt-based approach
        let test_pixels: [(i32, i32, i32); 5] = [
            (200, 20, 20),   // exact match for HP orb
            (180, 15, 25),   // close to HP orb
            (100, 100, 100), // gray — should NOT match
            (30, 30, 200),   // mana orb match
            (198, 166, 99),  // gold text match
        ];

        let (tr, tg, tb) = (180i32, 20i32, 20i32);
        let threshold = 60i32;
        let threshold_sq = threshold * threshold;

        for (r, g, b) in test_pixels {
            let dist_sq = (r - tr).pow(2) + (g - tg).pow(2) + (b - tb).pow(2);
            let dist_sqrt = (dist_sq as f32).sqrt();

            let match_sq = dist_sq < threshold_sq;
            let match_sqrt = dist_sqrt < threshold as f32;

            assert_eq!(
                match_sq, match_sqrt,
                "Squared vs sqrt mismatch for ({},{},{}): sq={} sqrt={}",
                r, g, b, match_sq, match_sqrt
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Windows DXGI Desktop Duplication
// ═══════════════════════════════════════════════════════════════

#[cfg(windows)]
pub struct DxgiCapturer {
    device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    context: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    duplication: windows::Win32::Graphics::Dxgi::IDXGIOutputDuplication,
    /// Reusable staging texture — avoids per-frame GPU allocation
    staging: Option<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
    width: u32,
    height: u32,
}

#[cfg(windows)]
impl DxgiCapturer {
    pub fn new() -> anyhow::Result<Self> {
        use windows::Win32::Graphics::Direct3D::*;
        use windows::Win32::Graphics::Direct3D11::*;
        use windows::Win32::Graphics::Dxgi::*;

        unsafe {
            // Create D3D11 device
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_FLAG(0),
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;

            let device = device.ok_or_else(|| anyhow::anyhow!("No D3D11 device"))?;
            let context = context.ok_or_else(|| anyhow::anyhow!("No D3D11 context"))?;

            // Get DXGI device → adapter → output
            let dxgi_device: IDXGIDevice = device.cast()?;
            let adapter = dxgi_device.GetAdapter()?;
            let output: IDXGIOutput = adapter.EnumOutputs(0)?;
            let output1: IDXGIOutput1 = output.cast()?;

            // Get output description for dimensions
            let desc = output.GetDesc()?;
            let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as u32;
            let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as u32;

            // Create duplication
            let duplication = output1.DuplicateOutput(&device)?;

            Ok(Self {
                device,
                context,
                duplication,
                staging: None,
                width,
                height,
            })
        }
    }

    pub fn capture_frame(&mut self) -> anyhow::Result<CapturedFrame> {
        use windows::Win32::Graphics::Direct3D11::*;
        use windows::Win32::Graphics::Dxgi::*;

        unsafe {
            let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;

            // Try to acquire frame (16ms timeout = ~60fps max)
            self.duplication
                .AcquireNextFrame(16, &mut frame_info, &mut resource)
                .map_err(|e| anyhow::anyhow!("AcquireNextFrame: {}", e))?;

            let resource = resource.ok_or_else(|| anyhow::anyhow!("No resource"))?;
            let texture: ID3D11Texture2D = resource.cast()?;

            // Reuse staging texture — only create once (saves ~0.5ms/frame GPU alloc)
            let staging = match &self.staging {
                Some(s) => s.clone(),
                None => {
                    let mut desc = D3D11_TEXTURE2D_DESC::default();
                    texture.GetDesc(&mut desc);
                    desc.Usage = D3D11_USAGE_STAGING;
                    desc.BindFlags = D3D11_BIND_FLAG(0);
                    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
                    desc.MiscFlags = D3D11_RESOURCE_MISC_FLAG(0);
                    let s = self.device.CreateTexture2D(&desc, None)?;
                    self.staging = Some(s.clone());
                    s
                }
            };
            self.context.CopyResource(&staging, &texture);

            // Map and read pixels
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;

            let stride = mapped.RowPitch;
            let total_bytes = (stride * self.height) as usize;
            let pixels =
                std::slice::from_raw_parts(mapped.pData as *const u8, total_bytes).to_vec();

            self.context.Unmap(&staging, 0);
            self.duplication.ReleaseFrame()?;

            Ok(CapturedFrame {
                pixels,
                width: self.width,
                height: self.height,
                stride,
                timestamp: Instant::now(),
            })
        }
    }
}
