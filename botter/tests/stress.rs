// tests/stress.rs — Heavy stress tests for production validation
// Run with: cargo test --test stress -- --nocapture

use kzb_vision_agent::config::AgentConfig;
use kzb_vision_agent::decision::{Action, DecisionEngine};
use kzb_vision_agent::native_messaging::{NativeMessagingHost, SharedAgentStats};
use kzb_vision_agent::stealth::*;
use kzb_vision_agent::vision::*;

use rand::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════
// 1. SUSTAINED AGENT LOOP — 10 seconds @ 25Hz
// ═══════════════════════════════════════════════════════════════

#[test]
fn stress_sustained_agent_loop_10s() {
    let buffer = Arc::new(ShardedFrameBuffer::new());
    let stats = Arc::new(SharedAgentStats::new());
    let shutdown = Arc::new(AtomicBool::new(false));

    // Producer: 25Hz capture simulation
    let buf_w = Arc::clone(&buffer);
    let shut_w = Arc::clone(&shutdown);
    let producer = std::thread::spawn(move || {
        let mut rng = thread_rng();
        let mut tick = 0u64;
        let interval = Duration::from_millis(40); // 25 Hz
        while !shut_w.load(Ordering::Acquire) {
            let mut state = FrameState::default();
            state.tick = tick;
            state.hp_pct = rng.gen_range(25..100);
            state.mana_pct = rng.gen_range(10..100);
            state.enemy_count = if tick % 50 < 35 { rng.gen_range(1..10) } else { 0 };
            state.in_combat = state.enemy_count > 0;
            state.in_town = tick % 200 < 15;
            state.char_screen_x = 400;
            state.char_screen_y = 300;
            state.merc_alive = rng.gen::<f32>() > 0.1;

            // Occasional loot
            if !state.in_combat && tick % 30 == 0 {
                state.loot_label_count = rng.gen_range(1..4).min(MAX_LOOT_LABELS as u8);
                for i in 0..state.loot_label_count as usize {
                    state.loot_labels[i] = LootLabel {
                        x: rng.gen_range(100..700),
                        y: rng.gen_range(100..500),
                        quality: match rng.gen_range(0..5) {
                            0 => ItemQuality::Unique,
                            1 => ItemQuality::Set,
                            2 => ItemQuality::Rare,
                            3 => ItemQuality::Rune,
                            _ => ItemQuality::Magic,
                        },
                        text_hash: rng.gen(),
                    };
                }
            }

            buf_w.push(state);
            tick += 1;
            std::thread::sleep(interval);
        }
        tick
    });

    // Consumer: decision engine reading + deciding at 25Hz
    let buf_r = Arc::clone(&buffer);
    let stats_r = Arc::clone(&stats);
    let shut_r = Arc::clone(&shutdown);
    let consumer = std::thread::spawn(move || {
        let mut engine = DecisionEngine::new(AgentConfig::default());
        let mut decisions = 0u64;
        let mut chickens = 0u64;
        let mut attacks = 0u64;
        let mut potions = 0u64;
        let mut loots = 0u64;
        let mut moves = 0u64;
        let mut missed = 0u64;

        while !shut_r.load(Ordering::Acquire) {
            match buf_r.latest() {
                Some(state) => {
                    let d = engine.decide(&state);
                    decisions += 1;
                    stats_r.decisions_made.fetch_add(1, Ordering::Relaxed);

                    match d.action {
                        Action::ChickenQuit => chickens += 1,
                        Action::CastSkill { .. } => attacks += 1,
                        Action::DrinkPotion { .. } => {
                            potions += 1;
                            stats_r.potions_used.fetch_add(1, Ordering::Relaxed);
                        }
                        Action::PickupLoot { .. } => {
                            loots += 1;
                            stats_r.loots_picked.fetch_add(1, Ordering::Relaxed);
                        }
                        Action::MoveTo { .. } => moves += 1,
                        _ => {}
                    }

                    // Respect the humanized delay (capped for test speed)
                    let delay = d.delay.min(Duration::from_millis(50));
                    if delay > Duration::ZERO {
                        std::thread::sleep(delay);
                    }
                }
                None => {
                    missed += 1;
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }

        (decisions, chickens, attacks, potions, loots, moves, missed)
    });

    // Run for 10 seconds
    std::thread::sleep(Duration::from_secs(10));
    shutdown.store(true, Ordering::Release);

    let frames_written = producer.join().unwrap();
    let (decisions, chickens, attacks, potions, loots, moves, missed) = consumer.join().unwrap();

    println!("\n=== SUSTAINED AGENT LOOP (10s @ 25Hz) ===");
    println!("  Frames captured:  {}", frames_written);
    println!("  Decisions made:   {}", decisions);
    println!("  Missed frames:    {}", missed);
    println!("  Attacks:          {}", attacks);
    println!("  Potions:          {}", potions);
    println!("  Loots:            {}", loots);
    println!("  Moves:            {}", moves);
    println!("  Chickens:         {}", chickens);
    println!("  Decisions/sec:    {:.0}", decisions as f64 / 10.0);
    println!("  Buffer stats:     {:?}", buffer.stats());

    assert!(frames_written >= 200, "should write 250 frames in 10s, got {}", frames_written);
    assert!(decisions >= 50, "should decide many times, got {}", decisions);
    assert!(attacks + potions + loots + moves > 20, "should have combat actions");

    // SharedAgentStats should match
    let stat_decisions = stats.decisions_made.load(Ordering::Relaxed);
    assert_eq!(stat_decisions, decisions, "stats mismatch");
}

// ═══════════════════════════════════════════════════════════════
// 2. BUFFER RACE — 8 readers vs 1 writer, zero corruption
// ═══════════════════════════════════════════════════════════════

#[test]
fn stress_buffer_8_readers_zero_corruption() {
    let buffer = Arc::new(ShardedFrameBuffer::new());
    let shutdown = Arc::new(AtomicBool::new(false));

    // Writer at 25Hz
    let buf_w = Arc::clone(&buffer);
    let shut_w = Arc::clone(&shutdown);
    let writer = std::thread::spawn(move || {
        let mut tick = 0u64;
        while !shut_w.load(Ordering::Acquire) {
            let mut state = FrameState::default();
            state.tick = tick;
            state.hp_pct = (tick % 100) as u8;
            state.mana_pct = ((tick * 3) % 100) as u8;
            state.enemy_count = (tick % 10) as u8;
            buf_w.push(state);
            tick += 1;
            std::thread::sleep(Duration::from_micros(100));
        }
        tick
    });

    // 8 concurrent readers
    let total_corruptions = Arc::new(AtomicU64::new(0));
    let total_reads = Arc::new(AtomicU64::new(0));
    let mut readers = Vec::new();

    for _ in 0..8 {
        let buf_r = Arc::clone(&buffer);
        let shut_r = Arc::clone(&shutdown);
        let corr = Arc::clone(&total_corruptions);
        let reads = Arc::clone(&total_reads);

        readers.push(std::thread::spawn(move || {
            while !shut_r.load(Ordering::Acquire) {
                if let Some(state) = buf_r.latest() {
                    reads.fetch_add(1, Ordering::Relaxed);
                    // Verify consistency
                    if state.hp_pct != (state.tick % 100) as u8 {
                        corr.fetch_add(1, Ordering::Relaxed);
                    }
                    if state.mana_pct != ((state.tick * 3) % 100) as u8 {
                        corr.fetch_add(1, Ordering::Relaxed);
                    }
                    if state.enemy_count != (state.tick % 10) as u8 {
                        corr.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    std::thread::sleep(Duration::from_secs(5));
    shutdown.store(true, Ordering::Release);

    let writes = writer.join().unwrap();
    for r in readers { r.join().unwrap(); }

    let corruptions = total_corruptions.load(Ordering::Relaxed);
    let reads = total_reads.load(Ordering::Relaxed);

    println!("\n=== BUFFER RACE: 8 READERS vs 1 WRITER (5s) ===");
    println!("  Writes:       {}", writes);
    println!("  Total reads:  {}", reads);
    println!("  Corruptions:  {}", corruptions);
    println!("  Reads/sec:    {:.0}", reads as f64 / 5.0);

    assert_eq!(corruptions, 0, "ZERO corruption required");
    assert!(reads > 10_000, "should have many reads: {}", reads);
}

// ═══════════════════════════════════════════════════════════════
// 3. THREAD INPUT POOL — 10K commands, verify distribution
// ═══════════════════════════════════════════════════════════════

#[test]
fn stress_thread_input_10k_commands() {
    let pool = ThreadRotatedInput::new(ThreadPoolConfig {
        num_workers: 4,
        timing_jitter_mean_us: 50.0,  // low jitter for speed
        timing_jitter_stddev_us: 20.0,
        strategy: RotationStrategy::RoundRobin,
        ..Default::default()
    });

    let start = Instant::now();
    let count = 10_000u32;

    for i in 0..count {
        let cmd = match i % 5 {
            0 => InputCommand::KeyPress { key: (b'a' + (i % 26) as u8) as char, hold_ms: 5 },
            1 => InputCommand::MouseMove { x: (i % 800) as i32, y: (i % 600) as i32 },
            2 => InputCommand::LeftClick { hold_ms: 5 },
            3 => InputCommand::RightClick { hold_ms: 5 },
            _ => InputCommand::ClickAt {
                x: (i % 800) as i32, y: (i % 600) as i32,
                button: MouseButton::Left, hold_ms: 5,
            },
        };
        pool.dispatch(cmd);
    }

    // Wait for drain
    std::thread::sleep(Duration::from_secs(8));
    let elapsed = start.elapsed();
    let stats = pool.stats();
    let total: u64 = stats.per_thread.iter().map(|t| t.commands_processed).sum();

    println!("\n=== THREAD INPUT: 10K COMMANDS ===");
    println!("  Dispatched: {}", count);
    println!("  Processed:  {}", total);
    println!("  Elapsed:    {:?}", elapsed);
    for ts in &stats.per_thread {
        println!("    Thread {}: {} cmds, avg latency {}µs",
            ts.thread_id, ts.commands_processed, ts.avg_latency_us);
    }

    // All 4 threads should be active
    let active = stats.per_thread.iter().filter(|t| t.commands_processed > 0).count();
    assert_eq!(active, 4, "all threads should be active");

    // Distribution should be roughly even across threads
    let per_thread_expected = total / 4;
    for ts in &stats.per_thread {
        let deviation = (ts.commands_processed as i64 - per_thread_expected as i64).unsigned_abs();
        assert!(
            deviation < per_thread_expected / 2,
            "thread {} got {} (expected ~{})", ts.thread_id, ts.commands_processed, per_thread_expected
        );
    }

    assert!(total >= 1000, "should process many commands: {}", total);
}

// ═══════════════════════════════════════════════════════════════
// 4. DECISION ENGINE — edge cases that could crash
// ═══════════════════════════════════════════════════════════════

#[test]
fn stress_decision_edge_cases() {
    let mut engine = DecisionEngine::new(AgentConfig::default());
    let mut rng = thread_rng();

    // Test with extreme values
    let edge_cases: Vec<FrameState> = vec![
        // All zeros
        FrameState::default(),
        // Max HP, no enemies
        {
            let mut s = FrameState::default();
            s.hp_pct = 100;
            s.mana_pct = 100;
            s
        },
        // Zero HP (should chicken)
        {
            let mut s = FrameState::default();
            s.hp_pct = 0;
            s.in_combat = true;
            s.enemy_count = 1;
            s
        },
        // Max enemies
        {
            let mut s = FrameState::default();
            s.hp_pct = 80;
            s.enemy_count = 255;
            s.in_combat = true;
            s
        },
        // Full loot array
        {
            let mut s = FrameState::default();
            s.loot_label_count = MAX_LOOT_LABELS as u8;
            for i in 0..MAX_LOOT_LABELS {
                s.loot_labels[i] = LootLabel {
                    x: (i * 50) as u16,
                    y: (i * 30) as u16,
                    quality: ItemQuality::Unique,
                    text_hash: i as u32,
                };
            }
            s
        },
        // In town with full HP
        {
            let mut s = FrameState::default();
            s.hp_pct = 100;
            s.mana_pct = 100;
            s.in_town = true;
            s
        },
    ];

    println!("\n=== DECISION ENGINE EDGE CASES ===");
    for (i, state) in edge_cases.iter().enumerate() {
        // Reset cooldowns
        
        
        
        

        let d = engine.decide(state);
        println!("  Case {}: hp={} mana={} enemies={} combat={} town={} → {:?} (delay {:?})",
            i, state.hp_pct, state.mana_pct, state.enemy_count,
            state.in_combat, state.in_town, d.action, d.delay);
    }

    // Random fuzz: 100K random states, none should panic
    let start = Instant::now();
    for _ in 0..100_000 {
        let mut state = FrameState::default();
        state.hp_pct = rng.gen();
        state.mana_pct = rng.gen();
        state.enemy_count = rng.gen();
        state.in_combat = rng.gen();
        state.in_town = rng.gen();
        state.merc_alive = rng.gen();
        state.loot_label_count = rng.gen_range(0..=MAX_LOOT_LABELS as u8);
        state.char_screen_x = rng.gen();
        state.char_screen_y = rng.gen();

        let _ = engine.decide(&state);
    }
    let elapsed = start.elapsed();
    println!("  100K random fuzzes: {:?} ({:.0} decisions/sec)",
        elapsed, 100_000.0 / elapsed.as_secs_f64());
}

// ═══════════════════════════════════════════════════════════════
// 5. NATIVE MESSAGING — concurrent stats hammering
// ═══════════════════════════════════════════════════════════════

#[test]
fn stress_native_messaging_concurrent_stats() {
    let stats = Arc::new(SharedAgentStats::new());
    let buffer = Arc::new(ShardedFrameBuffer::new());
    let shutdown = Arc::new(AtomicBool::new(false));

    // 4 threads hammering stats
    let mut writers = Vec::new();
    for _ in 0..4 {
        let s = Arc::clone(&stats);
        let shut = Arc::clone(&shutdown);
        writers.push(std::thread::spawn(move || {
            let mut count = 0u64;
            while !shut.load(Ordering::Acquire) {
                s.frames_processed.fetch_add(1, Ordering::Relaxed);
                s.decisions_made.fetch_add(1, Ordering::Relaxed);
                s.potions_used.fetch_add(1, Ordering::Relaxed);
                s.loots_picked.fetch_add(1, Ordering::Relaxed);
                s.chickens.fetch_add(1, Ordering::Relaxed);
                s.kills.fetch_add(1, Ordering::Relaxed);
                s.deaths.fetch_add(1, Ordering::Relaxed);
                count += 1;
            }
            count
        }));
    }

    // Host reading stats simultaneously
    let (host, _cmd_rx) = NativeMessagingHost::new(
        Arc::clone(&stats),
        Arc::clone(&buffer),
    );

    let host_shut = Arc::clone(&shutdown);
    let reader = std::thread::spawn(move || {
        let mut snapshots = 0u64;
        while !host_shut.load(Ordering::Acquire) {
            let resp = host.handle_message(serde_json::json!({"cmd": "get_stats"}));
            assert!(resp.is_some());
            snapshots += 1;
        }
        snapshots
    });

    std::thread::sleep(Duration::from_secs(3));
    shutdown.store(true, Ordering::Release);

    let mut total_writes = 0u64;
    for w in writers { total_writes += w.join().unwrap(); }
    let snapshots = reader.join().unwrap();

    let frames = stats.frames_processed.load(Ordering::Relaxed);
    let decisions = stats.decisions_made.load(Ordering::Relaxed);

    println!("\n=== NATIVE MESSAGING: CONCURRENT STATS (3s) ===");
    println!("  Writer threads: 4 × {} = {} total ops per counter", total_writes / 4, total_writes);
    println!("  Frames counter:  {}", frames);
    println!("  JSON snapshots:  {}", snapshots);
    println!("  Snapshots/sec:   {:.0}", snapshots as f64 / 3.0);

    assert_eq!(frames, total_writes, "frames counter mismatch");
    assert_eq!(decisions, total_writes, "decisions counter mismatch");
    assert!(snapshots > 1000, "should have many snapshots: {}", snapshots);
}

// ═══════════════════════════════════════════════════════════════
// 6. STEALTH CADENCE — sustained decoy injection
// ═══════════════════════════════════════════════════════════════

#[test]
fn stress_cadence_sustained_decoys() {
    let mut cadence = SyscallCadence::new(CadenceConfig {
        
        ..Default::default()
    });

    let categories = [
        SyscallCategory::ScreenCapture,
        SyscallCategory::InputDispatch,
        SyscallCategory::FileIO,
        SyscallCategory::TimerWait,
        SyscallCategory::Memory,
    ];

    let start = Instant::now();
    let mut total_calls = 0u64;

    for _ in 0..50_000 {
        let cat = categories[total_calls as usize % categories.len()];
        let prep = cadence.pre_syscall(cat);
        cadence.execute_decoys(&prep);
        total_calls += 1;
    }
    let elapsed = start.elapsed();

    let report = cadence.get_stats();

    println!("\n=== CADENCE: 50K SYSCALLS WITH DECOYS ===");
    println!("  Total calls:    {}", total_calls);
    println!("  Total decoys:   {}", report.total_decoys_executed);
    println!("  Elapsed:        {:?}", elapsed);
    println!("  Calls/sec:      {:.0}", total_calls as f64 / elapsed.as_secs_f64());
    println!("  Screen capture: calls={} avg_jitter={}µs",
        report.screen_capture.call_count, report.screen_capture.avg_jitter_us);
    println!("  Input dispatch: calls={} avg_jitter={}µs",
        report.input_dispatch.call_count, report.input_dispatch.avg_jitter_us);

    assert!(report.total_decoys_executed > 1000, "should have many decoys: {}",
        report.total_decoys_executed);
    assert!(report.screen_capture.avg_jitter_us > 0, "should have non-zero jitter");
}

// ═══════════════════════════════════════════════════════════════
// 7. CAPTURE PIPELINE — vision extraction from synthetic frames
// ═══════════════════════════════════════════════════════════════

#[test]
fn stress_capture_vision_extraction() {
    use kzb_vision_agent::vision::capture::{CaptureConfig, CapturePipeline};

    let buffer = Arc::new(ShardedFrameBuffer::new());
    let config = CaptureConfig::default();
    let mut pipeline = CapturePipeline::new(config, Arc::clone(&buffer));
    let running = pipeline.running_flag();

    // Run capture for 3 seconds
    let handle = std::thread::spawn(move || {
        pipeline.run();
    });

    std::thread::sleep(Duration::from_secs(3));
    running.store(false, Ordering::Release);
    handle.join().unwrap();

    let stats = buffer.stats();
    println!("\n=== CAPTURE PIPELINE (3s) ===");
    println!("  Frames written:  {}", stats.total_frames_written);
    println!("  Shards complete: {}", stats.shards_complete);

    // Should have ~75 frames (25Hz × 3s)
    assert!(stats.total_frames_written >= 50,
        "expected ~75 frames, got {}", stats.total_frames_written);

    // Read and validate a frame
    if let Some(frame) = buffer.latest() {
        println!("  Latest frame: tick={} hp={}% mana={}% enemies={} town={} merc={}",
            frame.tick, frame.hp_pct, frame.mana_pct,
            frame.enemy_count, frame.in_town, frame.merc_alive);
    }
}

// ═══════════════════════════════════════════════════════════════
// 8. FULL PIPELINE — capture → decide → input → stats → host
// ═══════════════════════════════════════════════════════════════

#[test]
fn stress_full_pipeline_5s() {
    let buffer = Arc::new(ShardedFrameBuffer::new());
    let stats = Arc::new(SharedAgentStats::new());
    let shutdown = Arc::new(AtomicBool::new(false));

    // Capture thread
    let cap_buf = Arc::clone(&buffer);
    let cap_shut = Arc::clone(&shutdown);
    let capture = std::thread::spawn(move || {
        let config = kzb_vision_agent::vision::capture::CaptureConfig::default();
        let mut pipeline = kzb_vision_agent::vision::capture::CapturePipeline::new(
            config, cap_buf,
        );
        let running = pipeline.running_flag();
        let shut = cap_shut;
        std::thread::spawn(move || {
            while !shut.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(50));
            }
            running.store(false, Ordering::Release);
        });
        pipeline.run();
    });

    // Decision + input thread
    let dec_buf = Arc::clone(&buffer);
    let dec_stats = Arc::clone(&stats);
    let dec_shut = Arc::clone(&shutdown);
    let decision = std::thread::spawn(move || {
        let mut engine = DecisionEngine::new(AgentConfig::default());
        let pool = ThreadRotatedInput::new(ThreadPoolConfig {
            num_workers: 4,
            timing_jitter_mean_us: 50.0,
            timing_jitter_stddev_us: 20.0,
            ..Default::default()
        });
        let mut cadence = SyscallCadence::new(CadenceConfig::default());
        let mut decisions = 0u64;

        while !dec_shut.load(Ordering::Acquire) {
            if let Some(state) = dec_buf.latest() {
                let d = engine.decide(&state);
                dec_stats.decisions_made.fetch_add(1, Ordering::Relaxed);
                dec_stats.frames_processed.fetch_add(1, Ordering::Relaxed);
                decisions += 1;

                // Cadence jitter
                let prep = cadence.pre_syscall(SyscallCategory::InputDispatch);
                cadence.execute_decoys(&prep);

                // Dispatch through pool
                match d.action {
                    Action::CastSkill { key, screen_x, screen_y } => {
                        pool.dispatch(InputCommand::KeyPress { key, hold_ms: 10 });
                        pool.dispatch(InputCommand::ClickAt {
                            x: screen_x, y: screen_y,
                            button: MouseButton::Right, hold_ms: 10,
                        });
                    }
                    Action::DrinkPotion { belt_slot } => {
                        let key = (b'1' + belt_slot) as char;
                        pool.dispatch(InputCommand::KeyPress { key, hold_ms: 10 });
                        dec_stats.potions_used.fetch_add(1, Ordering::Relaxed);
                    }
                    Action::PickupLoot { screen_x, screen_y } => {
                        pool.dispatch(InputCommand::ClickAt {
                            x: screen_x, y: screen_y,
                            button: MouseButton::Left, hold_ms: 10,
                        });
                        dec_stats.loots_picked.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => {}
                }

                // 40ms tick
                std::thread::sleep(Duration::from_millis(40));
            } else {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        decisions
    });

    // Host querying stats
    let (host, _rx) = NativeMessagingHost::new(
        Arc::clone(&stats),
        Arc::clone(&buffer),
    );
    let host_shut = Arc::clone(&shutdown);
    let host_thread = std::thread::spawn(move || {
        let mut queries = 0u64;
        while !host_shut.load(Ordering::Acquire) {
            let _ = host.handle_message(serde_json::json!({"cmd": "get_stats"}));
            let _ = host.handle_message(serde_json::json!({"cmd": "get_buffer_stats"}));
            queries += 2;
            std::thread::sleep(Duration::from_millis(100));
        }
        queries
    });

    // Run for 5 seconds
    std::thread::sleep(Duration::from_secs(5));
    shutdown.store(true, Ordering::Release);

    capture.join().unwrap();
    let decisions = decision.join().unwrap();
    let queries = host_thread.join().unwrap();

    let buf_stats = buffer.stats();
    let frames = stats.frames_processed.load(Ordering::Relaxed);
    let potions = stats.potions_used.load(Ordering::Relaxed);
    let loots = stats.loots_picked.load(Ordering::Relaxed);

    println!("\n=== FULL PIPELINE (5s) ===");
    println!("  Frames captured: {}", buf_stats.total_frames_written);
    println!("  Decisions made:  {}", decisions);
    println!("  Potions used:    {}", potions);
    println!("  Loots picked:    {}", loots);
    println!("  Host queries:    {}", queries);

    assert!(buf_stats.total_frames_written >= 50, "capture should produce frames");
    assert!(decisions >= 20, "should have decisions: {}", decisions);
    assert!(queries >= 20, "host should query: {}", queries);
}
