// =============================================================================
// D2R Map Helper - Chrome Native Messaging Host
// =============================================================================
// Binary name: chrome_map_helper.exe (disguised as Chrome component)
// Registry: com.d2vision.map
//
// Architecture (same pattern as the vision agent):
//   Chrome Extension
//     ├── connectNative("com.d2vision.agent") → chrome_helper.exe (vision)
//     └── connectNative("com.d2vision.map")   → chrome_map_helper.exe (THIS)
//
// This host handles:
//   1. Reading D2R game state (seed, area, position, difficulty)
//   2. Generating/caching map collision data
//   3. Providing map data to the extension for overlay rendering
//   4. Heartbeat/stats reporting
// =============================================================================

mod offsets;
mod memory;
mod mapgen;
mod protocol;
mod discovery;
mod stealth;

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
        }
    }
}

fn main() {
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

        InboundCommand::Shutdown => {
            let _ = send_response("shutdown_ack", json!({}));
            std::thread::sleep(std::time::Duration::from_millis(100));
            std::process::exit(0);
        }
    }

    Ok(())
}
