pub mod config;
pub mod decision;
pub mod host_registry;
pub mod input;
pub mod native_messaging;
pub mod stealth;
pub mod training;
pub mod vision;

use config::AgentConfig;
use decision::GameManager;
use native_messaging::{AgentCommand, NativeMessagingHost, SharedAgentStats};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use stealth::*;
use vision::{CaptureConfig, CapturePipeline, ShardedFrameBuffer};

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F12};

// ═══════════════════════════════════════════════════════════════
// PRODUCTION AGENT BINARY
// Launched by Chrome via native messaging. Runs until pipe EOF.
// ═══════════════════════════════════════════════════════════════

/// Check if the kill hotkey (F12) is pressed
/// F12 is chosen because D2R doesn't use it and it works even when game has focus
#[cfg(windows)]
fn check_kill_hotkey() -> bool {
    unsafe {
        let f12_state = GetAsyncKeyState(VK_F12.0 as i32);
        // Check if key is currently pressed (high-order bit)
        f12_state as u16 & 0x8000 != 0
    }
}

#[cfg(not(windows))]
fn check_kill_hotkey() -> bool {
    false // Non-Windows: no hotkey support yet
}

#[tokio::main]
async fn main() {
    // Silent logging — no stdout except native messaging protocol
    let log_dir = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    tracing_subscriber::fmt()
        .with_env_filter("kzb_vision_agent=debug")
        .with_writer(move || {
            let path = log_dir.join("agent.log");
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap())
        })
        .with_ansi(false)
        .init();

    tracing::info!("Agent starting — PID {}", std::process::id());

    // ─── Load Host Registry ────────────────────────────────────
    let registry = match host_registry::HostRegistry::load_or_create() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Failed to load host registry: {}", e);
            std::process::exit(1);
        }
    };
    let host_name = registry.agent_host_name();
    tracing::info!("Using native host: {}", host_name);

    // ─── Load Config ───────────────────────────────────────────
    // Priority: CLI arg > KZB_CONFIG env var > config.yaml in data dir
    let config_path = resolve_config_path();
    let config = if config_path.exists() {
        match AgentConfig::load(&config_path) {
            Ok(c) => {
                tracing::info!(
                    "Loaded config: {} (class={}, build={})",
                    config_path.display(),
                    c.character_class,
                    c.build
                );
                c
            }
            Err(e) => {
                tracing::warn!("Config load failed: {}, using defaults", e);
                AgentConfig::default()
            }
        }
    } else {
        tracing::warn!(
            "Config not found at {}, writing defaults",
            config_path.display()
        );
        let default = AgentConfig::default();
        if let Err(e) = default.save(&config_path) {
            tracing::warn!("Could not write default config: {}", e);
        }
        default
    };

    // ─── Shared State ──────────────────────────────────────────
    let buffer = Arc::new(ShardedFrameBuffer::new());
    let stats = Arc::new(SharedAgentStats::new());
    let shutdown = Arc::new(AtomicBool::new(false));

    // ─── Stealth Stack ─────────────────────────────────────────
    // Chrome native messaging gives us legitimate process tree already.
    // Only apply PEB disguise on Windows if not launched by Chrome.
    #[cfg(windows)]
    {
        // Pick a random Chrome subprocess persona each launch so the process
        // never presents the same identity twice (Renderer, GPU, Network, etc.)
        let disguise = {
            use rand::Rng;
            let variants = [
                ChromeDisguise::Renderer,
                ChromeDisguise::UtilityAudio,
                ChromeDisguise::UtilityNetwork,
                ChromeDisguise::GpuProcess,
            ];
            variants[rand::thread_rng().gen_range(0..variants.len())]
        };
        tracing::info!("PEB persona: {:?}", disguise);
        let mut identity = ProcessIdentity::new(disguise);
        if identity.find_chrome_parent().is_none() {
            tracing::info!("No Chrome parent found — applying PEB disguise");
            if let Err(e) = identity.apply() {
                tracing::warn!("PEB disguise failed: {} (non-fatal)", e);
            }
        } else {
            tracing::info!("Chrome parent found — legitimate child process, skipping PEB");
        }
    }

    // Capture timing with jitter
    let capture_timing_config = CaptureTimingConfig {
        mode: CaptureMode::Burst,
        target_fps: 25.0,
        burst_min_frames: 20,
        burst_max_frames: 80,
        skip_rate: 0.02,
        long_pause_rate: 0.005,
        ..Default::default()
    };

    // Syscall cadence controller
    let cadence = Arc::new(std::sync::Mutex::new(SyscallCadence::new(
        CadenceConfig::default(),
    )));

    // Handle lifecycle manager
    let _handle_mgr = Arc::new(std::sync::Mutex::new(HandleManager::new(2000)));

    // ─── Native Messaging Host ─────────────────────────────────
    let (host, mut cmd_rx) = NativeMessagingHost::new(Arc::clone(&stats), Arc::clone(&buffer));

    // ─── Capture Thread ────────────────────────────────────────
    let capture_buffer = Arc::clone(&buffer);
    let _capture_stats = Arc::clone(&stats);
    let capture_shutdown = Arc::clone(&shutdown);
    let capture_display_config = config.game_display.clone();

    let _capture_handle = std::thread::Builder::new()
        .name("capture".into())
        .spawn(move || {
            let cap_config = CaptureConfig::default();
            let mut pipeline = CapturePipeline::with_display_config(
                cap_config,
                capture_buffer,
                capture_display_config,
            );
            let running = pipeline.running_flag();

            // Link to global shutdown
            let shutdown_ref = capture_shutdown;
            std::thread::spawn(move || {
                while !shutdown_ref.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                running.store(false, Ordering::Release);
            });

            pipeline.run();
            tracing::info!("Capture thread exiting");
        })
        .expect("spawn capture thread");

    // ─── Decision + Input Loop (blocking thread — CPU-bound) ──
    let loop_config = config.clone();
    let loop_buffer = Arc::clone(&buffer);
    let loop_stats = Arc::clone(&stats);
    let loop_shutdown = Arc::clone(&shutdown);
    let loop_cadence = Arc::clone(&cadence);

    let agent_loop = tokio::task::spawn_blocking(move || {
        // Enable progression mode for leveling/quest scripts
        let quest_state_path = data_dir().join("quest_state.json");
        let mut game_mgr = GameManager::with_progression(loop_config.clone(), quest_state_path);
        let _capture_timing = CaptureTiming::new(capture_timing_config);
        let logger = training::TrainingLogger::new(data_dir().join("training_logs"));

        // Thread-rotated input pool for stealth
        let input_pool = ThreadRotatedInput::new(ThreadPoolConfig {
            num_workers: 4,
            strategy: RotationStrategy::RoundRobin,
            ..Default::default()
        });

        let tick_interval = Duration::from_millis(40); // 25 Hz decision rate

        tracing::info!("Agent loop started — 25 Hz decision rate");

        while !loop_shutdown.load(Ordering::Acquire) {
            let tick_start = Instant::now();

            // Check for kill hotkey (F12)
            if check_kill_hotkey() {
                tracing::info!("Kill hotkey (F12) detected — shutting down");
                loop_shutdown.store(true, Ordering::Release);
                break;
            }

            // Check for Chrome commands
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    AgentCommand::Shutdown => {
                        tracing::info!("Shutdown command received from Chrome");
                        loop_shutdown.store(true, Ordering::Release);
                        break;
                    }
                    AgentCommand::Pause(reason) => {
                        tracing::info!("Paused: {}", reason);
                    }
                    AgentCommand::Resume => {
                        tracing::info!("Resumed");
                    }
                    AgentCommand::UpdateConfig(data) => {
                        tracing::info!("Config update received");
                        if let Ok(new_config) = serde_json::from_value::<AgentConfig>(data) {
                            game_mgr.reload_config(new_config);
                        }
                    }
                    AgentCommand::UpdateMapState {
                        area_id, area_name, player_x, player_y, map_seed, difficulty, pois,
                    } => {
                        game_mgr.apply_map_state(
                            area_id, area_name, player_x, player_y, map_seed, difficulty, &pois,
                        );
                    }
                }
            }

            // Skip decision if paused
            if loop_stats.paused.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }

            // Read latest frame from capture thread
            let state = match loop_buffer.latest() {
                Some(s) => s,
                None => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
            };

            // ─── Decision ──────────────────────────────────────
            let decision = game_mgr.decide(&state);
            loop_stats.decisions_made.fetch_add(1, Ordering::Relaxed);
            loop_stats.frames_processed.fetch_add(1, Ordering::Relaxed);

            // Log for training data
            logger.log(&state, &decision);

            // ─── Execute Action ────────────────────────────────
            // Apply humanized delay BEFORE action
            if decision.delay > Duration::ZERO {
                std::thread::sleep(decision.delay);
            }

            // Cadence jitter before syscall
            if let Ok(mut cad) = loop_cadence.try_lock() {
                let prep = cad.pre_syscall(SyscallCategory::InputDispatch);
                if prep.jitter > Duration::ZERO {
                    std::thread::sleep(prep.jitter);
                }
                cad.execute_decoys(&prep);
            }

            // Dispatch through thread-rotated pool
            match &decision.action {
                decision::Action::CastSkill {
                    key,
                    screen_x,
                    screen_y,
                } => {
                    input_pool.dispatch(InputCommand::KeyPress {
                        key: *key,
                        hold_ms: 40,
                    });
                    input_pool.dispatch(InputCommand::ClickAt {
                        x: *screen_x,
                        y: *screen_y,
                        button: MouseButton::Right,
                        hold_ms: 30,
                    });
                }
                decision::Action::DrinkPotion { belt_slot } => {
                    let key = match belt_slot {
                        0 => '1',
                        1 => '2',
                        2 => '3',
                        3 => '4',
                        _ => '1',
                    };
                    input_pool.dispatch(InputCommand::KeyPress { key, hold_ms: 35 });
                    loop_stats.potions_used.fetch_add(1, Ordering::Relaxed);
                }
                decision::Action::PickupLoot { screen_x, screen_y } => {
                    input_pool.dispatch(InputCommand::ClickAt {
                        x: *screen_x,
                        y: *screen_y,
                        button: MouseButton::Left,
                        hold_ms: 25,
                    });
                    loop_stats.loots_picked.fetch_add(1, Ordering::Relaxed);
                }
                decision::Action::MoveTo { screen_x, screen_y } => {
                    input_pool.dispatch(InputCommand::ClickAt {
                        x: *screen_x,
                        y: *screen_y,
                        button: MouseButton::Left,
                        hold_ms: 25,
                    });
                }
                decision::Action::ChickenQuit => {
                    tracing::warn!("CHICKEN — exiting game");
                    loop_stats.chickens.fetch_add(1, Ordering::Relaxed);
                    // Press Esc → Save & Exit (blocking sleep is fine here)
                    input_pool.dispatch(InputCommand::KeyPress {
                        key: '\x1b',
                        hold_ms: 40,
                    });
                    std::thread::sleep(Duration::from_millis(200));
                    input_pool.dispatch(InputCommand::KeyPress {
                        key: '\r',
                        hold_ms: 40,
                    });
                    std::thread::sleep(Duration::from_millis(3000));
                }
                decision::Action::TownPortal => {
                    input_pool.dispatch(InputCommand::KeyPress {
                        key: 't',
                        hold_ms: 40,
                    });
                }
                decision::Action::RecastBuff { key } => {
                    input_pool.dispatch(InputCommand::KeyPress {
                        key: *key,
                        hold_ms: 35,
                    });
                }
                decision::Action::TakeBreak { duration } => {
                    tracing::info!("Taking break: {:?}", duration);
                    std::thread::sleep(*duration);
                }
                decision::Action::IdlePause { duration } => {
                    std::thread::sleep(*duration);
                }
                decision::Action::Dodge { screen_x, screen_y } => {
                    // Dodge = fast move away from danger
                    input_pool.dispatch(InputCommand::ClickAt {
                        x: *screen_x,
                        y: *screen_y,
                        button: MouseButton::Left,
                        hold_ms: 20,
                    });
                }
                decision::Action::SwitchWeapon => {
                    // W key = weapon swap in D2R
                    input_pool.dispatch(InputCommand::KeyPress {
                        key: 'w',
                        hold_ms: 35,
                    });
                }
                decision::Action::Wait => {}
                decision::Action::Click { screen_x, screen_y } => {
                    input_pool.dispatch(InputCommand::ClickAt {
                        x: *screen_x,
                        y: *screen_y,
                        button: MouseButton::Left,
                        hold_ms: 25,
                    });
                }
            }

            // ─── Post-execution command flush ──────────────────
            // Drain commands that arrived during humanized delays,
            // jitter sleeps, and input dispatch so survival config
            // (chicken_hp_pct, hp_potion_pct, tp_retreat_pct) is
            // fresh before the next tick's decide(). Cuts worst-case
            // "missed chicken/potion" window from ~40ms down to ~5ms.
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    AgentCommand::Shutdown => {
                        loop_shutdown.store(true, Ordering::Release);
                    }
                    AgentCommand::UpdateConfig(data) => {
                        if let Ok(new_config) = serde_json::from_value::<AgentConfig>(data) {
                            game_mgr.reload_config(new_config);
                        }
                    }
                    AgentCommand::UpdateMapState {
                        area_id, area_name, player_x, player_y, map_seed, difficulty, pois,
                    } => {
                        game_mgr.apply_map_state(
                            area_id, area_name, player_x, player_y, map_seed, difficulty, &pois,
                        );
                    }
                    _ => {} // Pause/Resume handled at tick start
                }
            }

            // Precise tick timing
            let elapsed = tick_start.elapsed();
            if elapsed < tick_interval {
                std::thread::sleep(tick_interval - elapsed);
            }
        }

        tracing::info!("Agent loop exiting");
    });

    // ─── Native Messaging Stdio Loop ───────────────────────────
    // This runs on the main thread. Chrome communicates via stdin/stdout.
    // When Chrome closes, stdin EOF triggers shutdown.
    let stdio_shutdown = Arc::clone(&shutdown);

    let stdio_loop = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut read_buf = vec![0u8; 65536];

        tracing::info!("Native messaging stdio loop started");

        loop {
            // Read 4-byte length prefix
            let mut len_buf = [0u8; 4];
            match stdin.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(_) => {
                    // Pipe closed — Chrome exited
                    tracing::info!("Stdin EOF — Chrome disconnected");
                    break;
                }
            }

            let msg_len = u32::from_le_bytes(len_buf) as usize;
            if msg_len == 0 || msg_len > 1_048_576 {
                tracing::warn!("Invalid message length: {}", msg_len);
                continue;
            }

            // Read message body
            if msg_len > read_buf.len() {
                read_buf.resize(msg_len, 0);
            }
            match stdin.read_exact(&mut read_buf[..msg_len]).await {
                Ok(_) => {}
                Err(_) => break,
            }

            // Parse and handle
            let msg: serde_json::Value = match serde_json::from_slice(&read_buf[..msg_len]) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Invalid JSON: {}", e);
                    continue;
                }
            };

            if let Some(response) = host.handle_message(msg) {
                // Write response with 4-byte length prefix
                let resp_bytes = serde_json::to_vec(&response).unwrap_or_default();
                let resp_len = (resp_bytes.len() as u32).to_le_bytes();
                let _ = stdout.write_all(&resp_len).await;
                let _ = stdout.write_all(&resp_bytes).await;
                let _ = stdout.flush().await;
            }
        }

        tracing::info!("Stdio loop exiting — triggering shutdown");
        stdio_shutdown.store(true, Ordering::Release);
    });

    // ─── Wait for shutdown ─────────────────────────────────────
    tokio::select! {
        _ = agent_loop => tracing::info!("Agent loop finished"),
        _ = stdio_loop => tracing::info!("Stdio loop finished"),
    }

    // Graceful shutdown: give threads 2 seconds to clean up
    shutdown.store(true, Ordering::Release);
    tracing::info!("Shutdown signal sent — waiting 2s for cleanup");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Save final config (may have been updated via Chrome)
    // let _ = config.save(&config_path);

    tracing::info!("Agent exited cleanly");
}

/// Resolve which config YAML to load.
/// Priority: CLI arg (first arg) > KZB_CONFIG env var > data_dir/config.yaml
fn resolve_config_path() -> PathBuf {
    // Check CLI args: kzb_vision_agent.exe path/to/paladin_hammerdin.yaml
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && !args[1].starts_with('-') {
        let p = PathBuf::from(&args[1]);
        if p.exists() {
            return p;
        }
        // Try relative to data_dir/configs/
        let in_configs = data_dir().join("configs").join(&args[1]);
        if in_configs.exists() {
            return in_configs;
        }
        // Try adding .yaml extension
        let with_ext = data_dir()
            .join("configs")
            .join(format!("{}.yaml", &args[1]));
        if with_ext.exists() {
            return with_ext;
        }
    }

    // Check KZB_CONFIG env var
    if let Ok(env_path) = std::env::var("KZB_CONFIG") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            return p;
        }
    }

    // Default: config.yaml in data directory
    data_dir().join("config.yaml")
}

/// Data directory for config, logs, training data
fn data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"C:\ProgramData\DisplayCalibration")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/tmp/kzb_vision_agent")
    }
}

// ═══════════════════════════════════════════════════════════════
// INTEGRATION TESTS — run with `cargo test`
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use decision::DecisionEngine;
    use rand::Rng;

    #[tokio::test]
    async fn test_combat_simulation() {
        let buffer = Arc::new(ShardedFrameBuffer::new());
        let config = AgentConfig::default();
        let mut engine = DecisionEngine::new(config);
        let mut combat_actions = 0u32;

        for i in 0..500u64 {
            let mut state = vision::FrameState::default();
            state.tick = i;
            state.in_combat = i % 30 < 20;
            state.enemy_count = if state.in_combat {
                rand::thread_rng().gen_range(1..8)
            } else {
                0
            };
            state.hp_pct = if state.in_combat {
                rand::thread_rng().gen_range(40..100)
            } else {
                100
            };
            state.mana_pct = if state.in_combat {
                rand::thread_rng().gen_range(20..90)
            } else {
                100
            };
            state.char_screen_x = 640;
            state.char_screen_y = 360;
            buffer.push(state);

            engine.last_attack = Instant::now() - Duration::from_secs(5);
            engine.last_hp_potion = Instant::now() - Duration::from_secs(5);
            engine.last_mana_potion = Instant::now() - Duration::from_secs(5);
            engine.last_rejuv = Instant::now() - Duration::from_secs(5);

            if let Some(frame) = buffer.latest() {
                let d = engine.decide(&frame);
                match d.action {
                    decision::Action::CastSkill { .. }
                    | decision::Action::DrinkPotion { .. }
                    | decision::Action::MoveTo { .. }
                    | decision::Action::ChickenQuit => combat_actions += 1,
                    _ => {}
                }
            }
        }

        assert!(
            combat_actions > 100,
            "should have many combat actions: {}",
            combat_actions
        );
    }

    #[test]
    fn test_chicken_at_low_hp() {
        let buffer = ShardedFrameBuffer::new();
        let mut state = vision::FrameState::default();
        state.hp_pct = 20;
        state.in_combat = true;
        state.in_town = false;
        state.enemy_count = 5;
        buffer.push(state);

        let mut engine = DecisionEngine::new(AgentConfig::default());
        let frame = buffer.latest().unwrap();
        let d = engine.decide(&frame);

        assert!(matches!(d.action, decision::Action::ChickenQuit));
        assert_eq!(d.delay, Duration::ZERO);
    }

    #[test]
    fn test_delay_distributions() {
        let config = AgentConfig::default();
        let mut engine = DecisionEngine::new(config.clone());

        let mut survival = Vec::new();
        let mut normal = Vec::new();
        let mut attack = Vec::new();

        for _ in 0..5000 {
            survival.push(engine.survival_delay().as_millis() as f64);
            normal.push(engine.normal_delay().as_millis() as f64);
            attack.push(engine.attack_delay().as_millis() as f64);
        }

        let smax = survival.iter().cloned().fold(0.0f64, f64::max);
        let nmean = normal.iter().sum::<f64>() / normal.len() as f64;
        let amean = attack.iter().sum::<f64>() / attack.len() as f64;

        assert!(smax <= config.humanization.survival_max_delay_ms as f64);
        assert!(amean < nmean, "attack should be faster than normal");
    }

    #[test]
    fn test_buffer_throughput() {
        let buf = ShardedFrameBuffer::new();
        let total = 1_000_000u64;
        let start = Instant::now();

        for i in 0..total {
            let mut state = vision::FrameState::default();
            state.tick = i;
            buf.push(state);
        }

        let elapsed = start.elapsed();
        let throughput = total as f64 / elapsed.as_secs_f64();
        assert!(throughput > 500_000.0, "throughput: {:.0}", throughput);
    }

    #[test]
    fn test_config_yaml_roundtrip() {
        let config = AgentConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let loaded: AgentConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(loaded.character_class, "Sorceress");
        assert_eq!(loaded.survival.chicken_hp_pct, 30);
    }

    #[test]
    fn test_stealth_process_identity() {
        let mut identity = ProcessIdentity::new(ChromeDisguise::UtilityNetwork);
        identity.apply().unwrap();
        assert!(identity.is_applied());

        let cmd = ChromeDisguise::UtilityNetwork.command_line();
        assert!(cmd.contains("network.mojom.NetworkService"));

        identity.revert().unwrap();
        assert!(!identity.is_applied());
    }

    #[test]
    fn test_stealth_capture_timing() {
        let config = CaptureTimingConfig {
            mode: CaptureMode::Continuous,
            target_fps: 25.0,
            interval_jitter_ms: 6.0,
            skip_rate: 0.0,
            long_pause_rate: 0.0,
            ..Default::default()
        };
        let mut timing = CaptureTiming::new(config);
        let mut intervals = Vec::new();

        for _ in 0..5000 {
            if let CaptureAction::CaptureAndWait(d) = timing.next_action() {
                intervals.push(d.as_millis() as f64);
            }
        }

        let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
        assert!(mean > 37.0 && mean < 43.0, "mean: {:.1}", mean);
    }

    #[test]
    fn test_stealth_thread_input() {
        let pool = ThreadRotatedInput::new(ThreadPoolConfig {
            num_workers: 4,
            strategy: RotationStrategy::RoundRobin,
            ..Default::default()
        });

        for i in 0..100u32 {
            pool.dispatch(InputCommand::KeyPress {
                key: 'f',
                hold_ms: 5,
            });
        }

        std::thread::sleep(Duration::from_millis(2000));
        let stats = pool.stats();
        let total: u64 = stats.per_thread.iter().map(|t| t.commands_processed).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_native_messaging_protocol() {
        use native_messaging::test_pipe::*;

        let msg = serde_json::json!({"cmd": "ping"});
        let encoded = encode_message(&msg);
        let (decoded, consumed) = decode_message(&encoded).unwrap();
        assert_eq!(decoded["cmd"], "ping");
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_native_messaging_host() {
        let stats = Arc::new(SharedAgentStats::new());
        let buffer = Arc::new(ShardedFrameBuffer::new());
        stats.frames_processed.store(5000, Ordering::Relaxed);

        let (host, mut cmd_rx) = NativeMessagingHost::new(Arc::clone(&stats), Arc::clone(&buffer));

        // Handshake
        let resp = host
            .handle_message(serde_json::json!({"cmd": "handshake", "version": "1.0"}))
            .unwrap();
        assert_eq!(resp["cmd"], "handshake_ack");

        // Stats
        let resp = host
            .handle_message(serde_json::json!({"cmd": "get_stats"}))
            .unwrap();
        assert_eq!(resp["data"]["frames"], 5000);

        // Pause/Resume
        host.handle_message(serde_json::json!({"cmd": "pause", "reason": "test"}));
        assert!(stats.paused.load(Ordering::Relaxed));
        host.handle_message(serde_json::json!({"cmd": "resume"}));
        assert!(!stats.paused.load(Ordering::Relaxed));
    }

    #[test]
    fn test_shared_stats_concurrent() {
        let stats = Arc::new(SharedAgentStats::new());
        let mut handles = Vec::new();

        for _ in 0..8 {
            let s = Arc::clone(&stats);
            handles.push(std::thread::spawn(move || {
                for _ in 0..10000 {
                    s.frames_processed.fetch_add(1, Ordering::Relaxed);
                    s.decisions_made.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(stats.frames_processed.load(Ordering::Relaxed), 80000);
        assert_eq!(stats.decisions_made.load(Ordering::Relaxed), 80000);
    }

    #[test]
    fn test_full_pipeline() {
        let stats = Arc::new(SharedAgentStats::new());
        let buffer = Arc::new(ShardedFrameBuffer::new());
        let mut engine = DecisionEngine::new(AgentConfig::default());

        let (host, _cmd_rx) = NativeMessagingHost::new(Arc::clone(&stats), Arc::clone(&buffer));

        for tick in 0..500u64 {
            let mut state = vision::FrameState::default();
            state.tick = tick;
            state.hp_pct = 80;
            state.enemy_count = (tick % 5) as u8;
            state.in_combat = tick % 5 > 0;
            buffer.push(state);
            stats.frames_processed.fetch_add(1, Ordering::Relaxed);

            if let Some(frame) = buffer.latest() {
                let _ = engine.decide(&frame);
                stats.decisions_made.fetch_add(1, Ordering::Relaxed);
            }
        }

        let resp = host
            .handle_message(serde_json::json!({"cmd": "get_stats"}))
            .unwrap();
        assert_eq!(resp["data"]["frames"], 500);
        assert_eq!(resp["data"]["decisions"], 500);
    }
}
