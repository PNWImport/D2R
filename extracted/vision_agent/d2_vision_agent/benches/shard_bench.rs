use criterion::{criterion_group, criterion_main, Criterion};
use d2_vision_agent::vision::{FrameState, ShardedFrameBuffer};
use std::sync::Arc;

fn bench_push(c: &mut Criterion) {
    let buf = ShardedFrameBuffer::new();
    let state = FrameState::default();

    c.bench_function("shard_push", |b| {
        b.iter(|| buf.push(state.clone()))
    });
}

fn bench_latest(c: &mut Criterion) {
    let buf = ShardedFrameBuffer::new();
    // Pre-fill
    for i in 0..100 {
        let mut s = FrameState::default();
        s.tick = i;
        buf.push(s);
    }

    c.bench_function("shard_latest", |b| {
        b.iter(|| buf.latest())
    });
}

fn bench_recent(c: &mut Criterion) {
    let buf = ShardedFrameBuffer::new();
    for i in 0..100 {
        let mut s = FrameState::default();
        s.tick = i;
        buf.push(s);
    }

    c.bench_function("shard_recent_8", |b| {
        b.iter(|| buf.recent(8))
    });
}

criterion_group!(benches, bench_push, bench_latest, bench_recent);
criterion_main!(benches);
