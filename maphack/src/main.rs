// =============================================================================
// D2R Map Helper - Chrome Native Messaging Host
// =============================================================================
// Binary name: chrome_map_helper.exe (disguised as Chrome component)
// Registry: com.chromium.<random-12-16-hex> (regenerated per startup)
//
// Architecture (same pattern as the vision agent):
//   Chrome Extension
//     ├── connectNative("com.chromium.<hex>") → chrome_helper.exe (vision)
//     └── connectNative("com.chromium.<hex>") → chrome_map_helper.exe (THIS)
//
// This host handles:
//   1. Reading D2R game state (seed, area, position, difficulty)
//   2. Generating/caching map collision data
//   3. Providing map data to the extension for overlay rendering
//   4. Heartbeat/stats reporting
//   5. Button-activated mode (only responds when extension requests activation)
// =============================================================================

mod offsets;
mod memory;
mod mapgen;
mod protocol;
mod discovery;
mod stealth;
mod host_registry;

use protocol::*;
use memory::{ProcessReader, GameState};
use mapgen::MapGenerator;
use stealth::{ChromeDisguise, ProcessIdentity, CadenceConfig, SyscallCadence, SyscallCategory};
use serde_json::json;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct MapHelperState {
    reader: ProcessReader,
    generator: MapGenerator,
    cadence: SyscallCadence,
    enabled: bool,
    opacity: u8,
    last_state: Option<GameState>,
    poll_count: u64,
    // Button-activated mode
    map_active: bool,
    map_active_until: Option<std::time::Instant>,
}

impl MapHelperState {
    fn new() -> Self {
        Self {
            reader: ProcessReader::new(),
            generator: MapGenerator::new(),
            cadence: SyscallCadence::new(CadenceConfig::default()),
            enabled: true,
            opacity: 180,
            last_state: None,
            poll_count: 0,
            map_active: true,
            map_active_until: None,
        }
    }

    /// Check if map is currently active (considering auto-disable timer)
    fn is_map_active(&mut self) -> bool {
        // Check if auto-disable timer has expired
        if let Some(until) = self.map_active_until {
            if std::time::Instant::now() >= until {
                self.map_active = false;
                self.map_active_until = None;
            }
        }
        self.map_active
    }

    /// Activate map for specified duration (ms)
    fn activate_map(&mut self, duration_ms: u64) {
        if duration_ms > 0 {
            let duration = std::time::Duration::from_millis(duration_ms);
            self.map_active_until = Some(std::time::Instant::now() + duration);
        } else {
            self.map_active_until = None;
        }
        self.map_active = true;
    }

    /// Deactivate map immediately
    fn deactivate_map(&mut self) {
        self.map_active = false;
        self.map_active_until = None;
    }
}

fn main() {
    // Load or create host registry with random names
    let registry = match host_registry::HostRegistry::load_or_create() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[map_helper] Failed to load host registry: {}", e);
            std::process::exit(1);
        }
    };
    let host_name = registry.maphack_host_name();
    eprintln!("[map_helper] Using native host: {}", host_name);

    // Apply PEB disguise FIRST — before any other work
    let mut identity = ProcessIdentity::new(ChromeDisguise::UtilityNetwork);
    match identity.apply() {
        Ok(()) => eprintln!("[map_helper] PEB disguise applied (NetworkService)"),
        Err(e) => eprintln!("[map_helper] PEB disguise skipped: {}", e),
    }

    let mut state = MapHelperState::new();

    // Try to attach to game process on startup
    match state.reader.attach() {
        Ok(()) => eprintln!("[map_helper] Attached to game process"),
        Err(e) => eprintln!("[map_helper] Game not found (will retry): {}", e),
    }

    // Main message loop (blocking reads from Chrome extension via stdin)
    loop {
        match read_message() {
            Ok(Some(msg)) => {
                if let Err(e) = handle_message(&msg, &mut state) {
                    let _ = send_error("handle", &e);
                }
            }
            Ok(None) => {
                eprintln!("[map_helper] stdin closed, exiting");
                break;
            }
            Err(e) => {
                eprintln!("[map_helper] Read error: {}", e);
                break;
            }
        }
    }
}

fn handle_message(msg: &serde_json::Value, state: &mut MapHelperState) -> Result<(), String> {
    let cmd = parse_command(msg)?;

    match cmd {
        InboundCommand::Handshake { version } => {
            let _ = send_response("handshake_ack", json!({
                "version": VERSION,
                "type": "map_helper",
                "pid": std::process::id(),
                "ext_version": version,
                "d2r_attached": state.reader.is_attached(),
                "offsets_version": "MapAssist-compat-2026",
            }));
        }

        InboundCommand::Ping { timestamp } => {
            let _ = send_response("pong", json!({
                "timestamp": timestamp,
                "server_time": chrono::Utc::now().timestamp_millis(),
                "enabled": state.enabled,
                "poll_count": state.poll_count,
            }));
        }

        InboundCommand::ToggleMap { enabled } => {
            state.enabled = enabled;
            let _ = send_response("toggle_ack", json!({ "enabled": enabled }));
        }

        InboundCommand::ReadState => {
            if !state.enabled {
                let _ = send_response("state", json!({ "enabled": false, "in_game": false }));
                return Ok(());
            }

            // Check if map is currently active (auto-disable on timeout)
            if !state.is_map_active() {
                let _ = send_response("state", json!({ "enabled": false, "in_game": false, "reason": "map_inactive" }));
                return Ok(());
            }

            if !state.reader.is_attached()
                && state.reader.attach().is_err()
            {
                let _ = send_response("state", json!({
                    "d2r_attached": false, "error": "Game process not found"
                }));
                return Ok(());
            }

            // Pre-syscall: jitter + decoys before RPM reads
            let prep = state.cadence.pre_syscall(SyscallCategory::Memory);
            std::thread::sleep(prep.jitter);
            state.cadence.execute_decoys(&prep);

            match state.reader.read_game_state() {
                Ok(game_state) => {
                    state.last_state = Some(game_state.clone());
                    state.poll_count += 1;

                    let need_map = game_state.in_game && !game_state.is_town;
                    let map_data = if need_map {
                        let md = state.generator.get_map(
                            game_state.map_seed, game_state.area_id, game_state.difficulty,
                        );
                        Some(json!({
                            "width": md.width, "height": md.height,
                            "origin_x": md.origin_x, "origin_y": md.origin_y,
                            "poi_count": md.pois.len(), "pois": md.pois,
                            "collision_rows": md.collision_rows,
                            "collision_row_count": md.collision_rows.len(),
                        }))
                    } else { None };

                    let _ = send_response("state", json!({
                        "d2r_attached": true, "game_state": game_state,
                        "map": map_data, "opacity": state.opacity,
                    }));

                    // Post-read jitter (on top of the existing jitter_delay_ms)
                    std::thread::sleep(std::time::Duration::from_millis(
                        memory::jitter_delay_ms()
                    ));
                }
                Err(e) => {
                    let _ = send_response("state", json!({
                        "d2r_attached": true, "in_game": false, "error": e,
                    }));
                }
            }
        }

        InboundCommand::GenerateMap { seed, area_id, difficulty } => {
            let md = state.generator.get_map(seed, area_id, difficulty);
            let _ = send_response("map_data", json!({
                "seed": md.seed, "area_id": md.area_id, "difficulty": md.difficulty,
                "width": md.width, "height": md.height,
                "origin_x": md.origin_x, "origin_y": md.origin_y,
                "pois": md.pois,
                "collision_row_count": md.collision_rows.len(),
                "collision_rows": md.collision_rows,
                "generated_at": md.generated_at,
            }));
        }

        InboundCommand::SetOpacity { opacity } => {
            state.opacity = opacity.max(10);
            let _ = send_response("opacity_ack", json!({ "opacity": state.opacity }));
        }

        InboundCommand::SetArea { area, difficulty } => {
            if let Some(ref mut gs) = state.last_state {
                gs.area_id = area;
                gs.difficulty = difficulty;
            }
            let _ = send_response("area_ack", json!({ "area": area, "difficulty": difficulty }));
        }

        InboundCommand::SetBackend { path } => {
            state.generator.set_backend(path.clone());
            let _ = send_response("backend_set", json!({ "path": path }));
        }

        InboundCommand::CacheStats => {
            let (cached, max) = state.generator.cache_stats();
            let _ = send_response("cache_stats", json!({
                "cached_maps": cached, "max_cache": max,
                "poll_count": state.poll_count, "enabled": state.enabled,
                "d2r_attached": state.reader.is_attached(),
            }));
        }

        InboundCommand::GetOffsets => {
            let off = offsets::D2ROffsets::default();
            let _ = send_response("offsets", json!({
                "offsets": off,
                "version": "MapAssist-compat-2026",
                "note": "Static fallback offsets. Sig-scan overrides on attach.",
            }));
        }

        InboundCommand::ActivateMap { duration_ms } => {
            state.activate_map(duration_ms);
            let actual_duration = duration_ms.max(1000); // At least 1 second
            eprintln!("[map_helper] Map activated for {} ms", actual_duration);
            let _ = send_response("activate_ack", json!({
                "activated": true,
                "duration_ms": actual_duration,
            }));
        }

        InboundCommand::DeactivateMap => {
            state.deactivate_map();
            eprintln!("[map_helper] Map deactivated");
            let _ = send_response("deactivate_ack", json!({ "deactivated": true }));
        }

        InboundCommand::Kill { reason } => {
            let reason = reason.unwrap_or_else(|| "manual_kill".to_string());
            eprintln!("[map_helper] Kill command received: {}", reason);
            let _ = send_response("kill_ack", json!({ "reason": reason }));
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::process::exit(0);
        }

        InboundCommand::Shutdown => {
            let _ = send_response("shutdown_ack", json!({}));
            std::thread::sleep(std::time::Duration::from_millis(100));
            std::process::exit(0);
        }
    }

    Ok(())
}
