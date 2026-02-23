//! QuadCache — Four orthogonal acceleration lanes adapted for the D2R bot.
//!
//! Original design: QuadDB query acceleration (0.4μs retrieval, 10,000× vs. vector RAG).
//! Adapted here for: farming run preloading, survival threshold flattening,
//! and hot-pattern telemetry for the optional LLM strategic wrapper.
//!
//! ## Lane Map
//!
//! | Lane | Original purpose    | Bot adaptation                                 |
//! |------|---------------------|------------------------------------------------|
//! | 2    | Structural / Topo   | Farm run scripts preloaded + indexed at startup |
//! | 3    | Metric range        | Survival thresholds flattened to plain fields   |
//! | 4    | Hot join / Intent   | Recurring (hp_bin × combat × phase) patterns   |
//! | 1    | Exact query memo    | Not used — game states are never pixel-identical|
//!
//! ## Pipeline in this context
//!
//! ```text
//! Frame arrives
//!     ↓
//! Lane 3 (ThresholdBins)   → O(1) field read, no config traversal
//!     ↓ not a survival emergency
//! Lane 4 (HotKey lookup)   → O(1) HashMap hit for common (hp_bin × combat) patterns
//!     ↓ cold miss → full DecisionEngine tree
//! Lane 2 (PreparedRun)     → O(1) run lookup when phase transitions to Farming
//! ```
//!
//! ## Memory footprint
//!
//! - Lane 2: ~10 runs × ~2 KB each  ≈  20 KB
//! - Lane 3: 12 u8/u64 fields       ≈  80 bytes
//! - Lane 4: ~100 hot keys × 16 B   ≈   2 KB
//! - Lane 1: (absent)
//! Total: ~22 KB — no Warden surface, all in agent-private heap.
//!
//! ## LLM Wrapper integration (openclaw)
//!
//! QuadCache acts as the retrieval tier. An optional LLM strategic layer sits above:
//!
//! ```text
//! QuadCache (0.4 μs deterministic)
//!     ↓  Span { run_history, phase, hp_bin, hit_rate, cold_misses }
//! openclaw LLM (~30 ms, non-critical path only)
//!     ↓  AgentCommand::UpdateConfig  (run sequence, threshold tweaks)
//! Agent loop applies via cmd_rx (already wired)
//! ```
//!
//! Critical-path survival decisions (chicken, potion) **never** touch the LLM.
//! The LLM only influences strategic choices: run selection, config suggestions,
//! break scheduling. This is exactly the QuadCache two-stage architecture.

use crate::config::{AgentConfig, FarmRun};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════
// LANE 2 — STRUCTURAL: farm run scripts indexed at startup
// ═══════════════════════════════════════════════════════════════

/// A FarmRun preloaded and annotated for O(1) lookup.
/// Indexed once at startup, reused every run cycle.
#[derive(Debug, Clone)]
pub struct PreparedRun {
    /// Position in the original farming.sequence
    pub index: usize,
    /// Full config for this run (cloned once, never re-parsed)
    pub farm_run: FarmRun,
    /// Which act this run belongs to (derived from name)
    pub act: u8,
    /// True if this is a boss-kill run (vs. area clear)
    pub is_boss: bool,
}

/// Derive act number from kolbot-style run name.
fn act_for_run(name: &str) -> u8 {
    match name {
        "Mausoleum" | "Pit" | "Andariel" | "CountessBoss" | "Tristram" => 1,
        "Arcane" | "Summoner" | "Duriel" | "Stony" | "Radament" => 2,
        "Mephisto" | "Travincal" | "Pindleskin" | "LowerKurast" | "Arachnid" => 3,
        "Izual" | "Diablo" | "Chaos" | "CS" => 4,
        "Baal" | "Worldstone" | "Nihlathak" | "Eldritch" | "WSK" => 5,
        _ => 1,
    }
}

fn is_boss_run(name: &str) -> bool {
    matches!(
        name,
        "Mephisto"
            | "Diablo"
            | "Baal"
            | "Andariel"
            | "Duriel"
            | "Nihlathak"
            | "Summoner"
            | "Izual"
            | "Travincal"
    )
}

// ═══════════════════════════════════════════════════════════════
// LANE 3 — METRIC RANGE: flattened survival thresholds
// ═══════════════════════════════════════════════════════════════

/// All survival thresholds flattened to plain numeric fields.
///
/// Replaces 11 `self.config.survival.*` traversals per tick with direct
/// field reads. Re-materialized on every `reload_config` — zero overhead
/// on the hot path.
#[derive(Debug, Clone, Copy)]
pub struct ThresholdBins {
    pub chicken_hp:             u8,
    pub chicken_mana:           u8,
    pub chicken_merc:           u8,
    pub rejuv_hp:               u8,
    pub rejuv_mana:             u8,
    pub hp_potion:              u8,
    pub mp_potion:              u8,
    pub tp_retreat:             u8,
    pub merc_hp:                u8,
    pub hp_potion_cooldown_ms:  u64,
    pub mp_potion_cooldown_ms:  u64,
    pub rejuv_cooldown_ms:      u64,
}

impl ThresholdBins {
    pub fn from_config(config: &AgentConfig) -> Self {
        Self {
            chicken_hp:            config.survival.chicken_hp_pct,
            chicken_mana:          config.survival.mana_chicken_pct,
            chicken_merc:          config.survival.merc_chicken_pct,
            rejuv_hp:              config.survival.hp_rejuv_pct,
            rejuv_mana:            config.survival.mana_rejuv_pct,
            hp_potion:             config.survival.hp_potion_pct,
            mp_potion:             config.survival.mana_potion_pct,
            tp_retreat:            config.survival.tp_retreat_pct,
            merc_hp:               config.survival.merc_hp_pct,
            hp_potion_cooldown_ms: config.survival.hp_potion_cooldown_ms,
            mp_potion_cooldown_ms: config.survival.mana_potion_cooldown_ms,
            rejuv_cooldown_ms:     config.survival.rejuv_cooldown_ms,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// LANE 4 — HOT JOINS: recurring (hp_bin × combat × phase) patterns
// ═══════════════════════════════════════════════════════════════

/// Coarse HP bin — collapses continuous hp_pct to 4 states.
/// Recomputed each tick from `ThresholdBins` (one subtraction + compare).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HpBin {
    /// At or below chicken threshold — must flee immediately
    Critical,
    /// Below hp_potion threshold — need a potion
    Low,
    /// 60%–hp_potion range — monitor, no immediate action
    Medium,
    /// Above 60% — healthy
    High,
}

impl HpBin {
    pub fn classify(hp_pct: u8, thresholds: &ThresholdBins) -> Self {
        if hp_pct <= thresholds.chicken_hp {
            Self::Critical
        } else if hp_pct <= thresholds.hp_potion {
            Self::Low
        } else if hp_pct <= 60 {
            Self::Medium
        } else {
            Self::High
        }
    }
}

/// Compressed decision context — key for Lane 4 lookup table.
///
/// Encodes the three dominant query dimensions that actually recur
/// in production farming sessions. Fits in a single u8 register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HotKey {
    pub hp_bin:    HpBin,
    pub in_combat: bool,
    pub has_loot:  bool,
}

impl HotKey {
    /// Telemetry span emitted to the LLM wrapper when a hot pattern fires.
    /// Follows the QuadCache Span format: self-describing, citable, no re-query needed.
    pub fn to_span_features(&self, hit_count: u64) -> SpanFeatures {
        SpanFeatures {
            hp_bin:     self.hp_bin,
            in_combat:  self.in_combat,
            has_loot:   self.has_loot,
            hit_count,
        }
    }
}

/// Span features emitted for LLM context (QuadCache two-stage architecture).
/// The LLM sees this instead of raw game memory — deterministic, citable.
#[derive(Debug, Clone, Copy)]
pub struct SpanFeatures {
    pub hp_bin:    HpBin,
    pub in_combat: bool,
    pub has_loot:  bool,
    /// How many times this exact pattern has fired this session
    pub hit_count: u64,
}

// ═══════════════════════════════════════════════════════════════
// QUADCACHE — unified four-lane store
// ═══════════════════════════════════════════════════════════════

pub struct QuadCache {
    // Lane 2: run name → prepared run
    pub runs: HashMap<String, PreparedRun>,
    /// Ordered sequence of enabled run names (resolved once at warm)
    pub run_sequence: Vec<String>,

    // Lane 3: flattened thresholds — hot path uses this directly
    pub thresholds: ThresholdBins,

    // Lane 4: hot pattern hit counters (telemetry + LLM span data)
    hot_hits: HashMap<HotKey, u64>,
    pub cold_misses: u64,
}

impl QuadCache {
    /// Warm all lanes from config at startup.
    ///
    /// Call once in `GameManager::new()`. Typical cost: ~5 μs.
    /// All subsequent decision-path access is O(1).
    pub fn warm(config: &AgentConfig) -> Self {
        // Lane 2: index all enabled FarmRun entries
        let mut runs = HashMap::new();
        let mut run_sequence = Vec::new();

        for (i, farm_run) in config.farming.sequence.iter().enumerate() {
            if !farm_run.enabled {
                continue;
            }
            let name = farm_run.name.clone();
            runs.insert(
                name.clone(),
                PreparedRun {
                    index: i,
                    farm_run: farm_run.clone(),
                    act: act_for_run(&name),
                    is_boss: is_boss_run(&name),
                },
            );
            run_sequence.push(name);
        }

        // Lane 3: flatten thresholds
        let thresholds = ThresholdBins::from_config(config);

        Self {
            runs,
            run_sequence,
            thresholds,
            hot_hits: HashMap::new(),
            cold_misses: 0,
        }
    }

    /// Re-flatten Lane 3 after a hot config reload.
    ///
    /// Called by both `reload_config` paths (tick-start drain + post-exec drain).
    /// O(1) — 12 field copies.
    pub fn reload_thresholds(&mut self, config: &AgentConfig) {
        self.thresholds = ThresholdBins::from_config(config);
    }

    /// Re-index Lane 2 after a farming.sequence change.
    ///
    /// Only needed if the run list itself changed (not just threshold edits).
    pub fn rewarm_runs(&mut self, config: &AgentConfig) {
        self.runs.clear();
        self.run_sequence.clear();

        for (i, farm_run) in config.farming.sequence.iter().enumerate() {
            if !farm_run.enabled {
                continue;
            }
            let name = farm_run.name.clone();
            self.runs.insert(
                name.clone(),
                PreparedRun {
                    index: i,
                    farm_run: farm_run.clone(),
                    act: act_for_run(&name),
                    is_boss: is_boss_run(&name),
                },
            );
            self.run_sequence.push(name);
        }
    }

    /// Look up a prepared run by name (Lane 2).
    pub fn get_run(&self, name: &str) -> Option<&PreparedRun> {
        self.runs.get(name)
    }

    /// Current run name at the given cycle index (Lane 2).
    pub fn run_at(&self, index: usize) -> Option<&str> {
        self.run_sequence
            .get(index % self.run_sequence.len().max(1))
            .map(String::as_str)
    }

    /// Record a Lane 4 hot-pattern hit. Returns span features for LLM wrapper.
    pub fn record_hit(&mut self, key: HotKey) -> SpanFeatures {
        let count = self.hot_hits.entry(key).or_insert(0);
        *count += 1;
        key.to_span_features(*count)
    }

    /// Record a cold miss — full decision tree was needed.
    pub fn record_miss(&mut self) {
        self.cold_misses += 1;
    }

    /// Lane 4 hit rate — useful for LLM context and diagnostics.
    pub fn hit_rate(&self) -> f32 {
        let hits: u64 = self.hot_hits.values().sum();
        let total = hits + self.cold_misses;
        if total == 0 { 0.0 } else { hits as f32 / total as f32 }
    }

    /// Top-N hot patterns — emitted to LLM wrapper as session context.
    ///
    /// The LLM receives these as Span objects and can suggest config changes
    /// (e.g., "hp_potion_pct is firing 800 times/hour — consider raising it").
    pub fn top_patterns(&self, n: usize) -> Vec<(HotKey, u64)> {
        let mut pairs: Vec<_> = self.hot_hits.iter().map(|(&k, &v)| (k, v)).collect();
        pairs.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        pairs.truncate(n);
        pairs
    }
}
