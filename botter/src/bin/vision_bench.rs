//! vision_bench — CPU-only vision pipeline benchmark
//!
//! Runs the full extract_frame_state() loop as fast as possible on synthetic
//! game frames and writes live timing data to `vision_bench_out.json` every
//! 500 ms so vision_perf.html can display it.
//!
//! Usage:
//!   cargo run --bin vision_bench --release [seconds=10] [out=PATH]
//!
//! Example:
//!   cargo run --bin vision_bench --release 30 ../extension/vision_bench_out.json

use kzb_vision_agent::vision::{CaptureConfig, CapturePipeline, ShardedFrameBuffer};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let duration_secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let out_path = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "../extension/vision_bench_out.json".to_string());

    println!("╔═══════════════════════════════════════════════════════╗");
    println!("║       KZB Vision Pipeline — CPU Benchmark             ║");
    println!("╠═══════════════════════════════════════════════════════╣");
    println!("║  Platform : CPU-only pixel analysis (no GPU compute)  ║");
    println!("║  Frame    : 800×600 synthetic game scene              ║");
    println!("║  Duration : {} seconds                                 ║", duration_secs);
    println!("║  Output   : {}  ║", &out_path[..out_path.len().min(40)]);
    println!("╚═══════════════════════════════════════════════════════╝\n");

    // ─── Setup ────────────────────────────────────────────────────────────
    let config = CaptureConfig::default();
    let buffer = Arc::new(ShardedFrameBuffer::new());
    let mut pipeline = CapturePipeline::new(config, buffer);

    // Build 5 different synthetic scenes to vary workload realistically
    let scenes: Vec<kzb_vision_agent::vision::CapturedFrame> = vec![
        CapturePipeline::synthetic_frame(0, 0, 100, true),   // In town, full HP
        CapturePipeline::synthetic_frame(3, 2, 80, false),   // Light combat
        CapturePipeline::synthetic_frame(7, 3, 60, false),   // Medium combat
        CapturePipeline::synthetic_frame(10, 4, 30, false),  // Heavy combat, low HP
        CapturePipeline::synthetic_frame(2, 0, 95, false),   // Clearing, no loot
    ];

    // ─── Warm-up pass ─────────────────────────────────────────────────────
    print!("Warming up... ");
    for i in 0..500 {
        let _ = pipeline.bench_extract(&scenes[i % scenes.len()]);
    }
    println!("done\n");

    // ─── Benchmark loop ────────────────────────────────────────────────────
    let deadline = Instant::now() + Duration::from_secs(duration_secs);
    let mut report_deadline = Instant::now() + Duration::from_millis(500);

    let mut total_frames: u64 = 0;
    let mut window_frames: u64 = 0;
    let mut window_start = Instant::now();

    // Per-tier timing accumulators (μs totals)
    let mut t1_total_us: u64 = 0; // HP + enemies + loot (every frame)
    let mut t2_total_us: u64 = 0; // town + merc + belt + UI (every 3rd)
    let mut t3_total_us: u64 = 0; // area + quest + xp (every 5th)

    let mut last_hz: f64 = 0.0;
    let mut last_t1: f64 = 0.0;
    let mut last_t2: f64 = 0.0;
    let mut last_t3: f64 = 0.0;
    let mut last_total: f64 = 0.0;

    // Enemy/loot detection results from last frame
    let mut last_enemies: u8 = 0;
    let mut last_hp: u8 = 0;

    println!("{:<10} {:>10} {:>12} {:>12} {:>12} {:>12}",
        "Tick", "Hz", "T1(HP+enemy)", "T2(merc/UI)", "T3(area/xp)", "Avg/frame");
    println!("{}", "─".repeat(75));

    while Instant::now() < deadline {
        let scene = &scenes[(total_frames as usize) % scenes.len()];

        // Tier 1 timing: orbs + enemies + loot
        let t1_start = Instant::now();
        let state = pipeline.bench_extract(scene);
        let frame_us = t1_start.elapsed().as_micros() as u64;

        // Approximate per-tier split based on tick phase
        // (mirrors the tiered logic in extract_frame_state)
        let tick = state.tick;
        t1_total_us += frame_us; // T1 always runs; over-counted on T2/T3 ticks

        // Separate T2/T3 contributions on their respective ticks
        if tick % 3 == 0 {
            // Rough split: T2 is ~20% of total on those frames
            let t2_share = frame_us * 20 / 100;
            t1_total_us = t1_total_us.saturating_sub(t2_share);
            t2_total_us += t2_share;
        }
        if tick % 5 == 0 {
            let t3_share = frame_us * 12 / 100;
            t1_total_us = t1_total_us.saturating_sub(t3_share);
            t3_total_us += t3_share;
        }

        last_enemies = state.enemy_count;
        last_hp = state.hp_pct;
        total_frames += 1;
        window_frames += 1;

        // Report every 500ms
        if Instant::now() >= report_deadline {
            let window_elapsed = window_start.elapsed().as_secs_f64();
            if window_elapsed > 0.0 {
                last_hz = window_frames as f64 / window_elapsed;
                last_t1 = if window_frames > 0 { t1_total_us as f64 / window_frames as f64 } else { 0.0 };
                last_t2 = if window_frames > 0 { t2_total_us as f64 / (window_frames as f64 / 3.0).max(1.0) } else { 0.0 };
                last_t3 = if window_frames > 0 { t3_total_us as f64 / (window_frames as f64 / 5.0).max(1.0) } else { 0.0 };
                last_total = if window_frames > 0 { (t1_total_us + t2_total_us + t3_total_us) as f64 / window_frames as f64 } else { 0.0 };

                println!("{:<10} {:>9.0}Hz {:>10.1}μs {:>10.1}μs {:>10.1}μs {:>10.1}μs",
                    total_frames, last_hz, last_t1, last_t2, last_t3, last_total);

                // Write JSON for Chrome viz
                write_json(
                    &out_path, last_hz, last_t1, last_t2, last_t3, last_total,
                    total_frames, last_enemies, last_hp, duration_secs,
                    Instant::now() > deadline,
                );

                // Reset window
                window_frames = 0;
                window_start = Instant::now();
                t1_total_us = 0;
                t2_total_us = 0;
                t3_total_us = 0;
                report_deadline = Instant::now() + Duration::from_millis(500);
            }
        }
    }

    // Final report
    println!("\n{}", "═".repeat(75));
    println!("FINAL: {:.0} Hz  avg {:.1} μs/frame  total {} frames in {}s",
        last_hz, last_total, total_frames, duration_secs);
    println!("CPU-only pixel math — zero GPU compute used.");
    println!("Results written to: {}", out_path);

    // Write final JSON with done=true
    write_json(&out_path, last_hz, last_t1, last_t2, last_t3, last_total,
        total_frames, last_enemies, last_hp, duration_secs, true);
}

fn write_json(
    path: &str,
    hz: f64,
    t1_us: f64,
    t2_us: f64,
    t3_us: f64,
    total_us: f64,
    total_frames: u64,
    enemies: u8,
    hp_pct: u8,
    duration: u64,
    done: bool,
) {
    let json = format!(
        r#"{{
  "generated": "{}",
  "cpu_only": true,
  "build": "{}",
  "frame_size": "800x600",
  "hz": {:.1},
  "frame_us": {:.2},
  "frame_ms": {:.3},
  "tier1_us": {:.2},
  "tier2_us": {:.2},
  "tier3_us": {:.2},
  "tier2_run_pct": 33,
  "tier3_run_pct": 20,
  "total_frames": {},
  "enemies_last": {},
  "hp_pct_last": {},
  "budget_ms": 40,
  "budget_used_pct": {:.1},
  "duration_secs": {},
  "done": {}
}}"#,
        chrono_now(),
        if cfg!(debug_assertions) { "debug" } else { "release" },
        hz,
        total_us,
        total_us / 1000.0,
        t1_us,
        t2_us,
        t3_us,
        total_frames,
        enemies,
        hp_pct,
        (total_us / 1000.0) / 40.0 * 100.0,
        duration,
        done,
    );

    if let Err(e) = std::fs::write(path, &json) {
        eprintln!("warn: could not write {}: {}", path, e);
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}", secs)
}
