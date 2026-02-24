use criterion::{criterion_group, criterion_main, Criterion};
use kzb_vision_agent::config::AgentConfig;
use kzb_vision_agent::decision::DecisionEngine;
use kzb_vision_agent::vision::{CaptureConfig, CapturePipeline, FrameState, ShardedFrameBuffer};
use std::sync::Arc;

fn bench_push(c: &mut Criterion) {
    let buf = ShardedFrameBuffer::new();
    let state = FrameState::default();

    c.bench_function("shard_push", |b| b.iter(|| buf.push(state.clone())));
}

fn bench_latest(c: &mut Criterion) {
    let buf = ShardedFrameBuffer::new();
    // Pre-fill
    for i in 0..100 {
        let mut s = FrameState::default();
        s.tick = i;
        buf.push(s);
    }

    c.bench_function("shard_latest", |b| b.iter(|| buf.latest()));
}

fn bench_recent(c: &mut Criterion) {
    let buf = ShardedFrameBuffer::new();
    for i in 0..100 {
        let mut s = FrameState::default();
        s.tick = i;
        buf.push(s);
    }

    c.bench_function("shard_recent_8", |b| b.iter(|| buf.recent(8)));
}

/// Full pipeline: pixel extract → QuadCache (ThresholdBins Lane 3 + HotKey Lane 4) → Decision
fn bench_decide(c: &mut Criterion) {
    let buf = Arc::new(ShardedFrameBuffer::new());
    let mut pipeline = CapturePipeline::new(CaptureConfig::default(), Arc::clone(&buf));
    let mut engine = DecisionEngine::new(AgentConfig::default());

    // Warm up QuadCache hot paths with varied frame patterns
    let scenes = [
        CapturePipeline::synthetic_frame(0, 0, 100, true),   // town / full HP
        CapturePipeline::synthetic_frame(3, 2, 80, false),   // light combat
        CapturePipeline::synthetic_frame(7, 3, 60, false),   // medium combat
        CapturePipeline::synthetic_frame(10, 4, 30, false),  // heavy combat, low HP
        CapturePipeline::synthetic_frame(2, 0, 95, false),   // clearing
    ];
    for scene in scenes.iter().cycle().take(200) {
        let state = pipeline.bench_extract(scene);
        let _ = engine.decide(&state);
    }

    // Bench the hot path: extract + decide together
    let mut i = 0usize;
    c.bench_function("vision_extract_plus_decide", |b| {
        b.iter(|| {
            let state = pipeline.bench_extract(&scenes[i % scenes.len()]);
            let decision = engine.decide(&state);
            i += 1;
            decision
        })
    });

    // Bench decide-only (QuadCache only, frame already extracted)
    let prebuilt: Vec<FrameState> = scenes
        .iter()
        .map(|s| pipeline.bench_extract(s))
        .collect();
    let mut j = 0usize;
    c.bench_function("quadcache_decide_only", |b| {
        b.iter(|| {
            let d = engine.decide(&prebuilt[j % prebuilt.len()]);
            j += 1;
            d
        })
    });
}

criterion_group!(benches, bench_push, bench_latest, bench_recent, bench_decide);
criterion_main!(benches);
