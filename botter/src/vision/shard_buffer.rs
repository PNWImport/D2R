//! 16-shard lock-free frame state buffer.
//!
//! Single-producer (vision capture thread) writes at 25 Hz.
//! Multiple consumers (decision engine, training logger) read latest state.
//! Zero contention: writer cycles through shards, readers only touch completed shards.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub const SHARD_COUNT: usize = 16;
const SHARD_MASK: usize = SHARD_COUNT - 1;

/// Shard status values
const STATUS_IDLE: u64 = 0;
const STATUS_WRITING: u64 = 1;
const STATUS_COMPLETE: u64 = 2;

/// Maximum loot labels per frame (stack-allocated, no heap alloc in hot path)
pub const MAX_LOOT_LABELS: usize = 8;

/// Item quality tiers — packed as u8 for cache-line friendliness.
/// Maps to D2 text colors detected by the vision pipeline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ItemQuality {
    #[default]
    Normal = 0,
    Superior = 1,
    Magic = 2,
    Rare = 3,
    Set = 4,
    Unique = 5,
    Rune = 6,
    Quest = 7,
}

impl ItemQuality {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Normal,
            1 => Self::Superior,
            2 => Self::Magic,
            3 => Self::Rare,
            4 => Self::Set,
            5 => Self::Unique,
            6 => Self::Rune,
            7 => Self::Quest,
            _ => Self::Normal,
        }
    }
}

/// A loot label detected on screen. Fixed-size, stack-allocated.
#[derive(Clone, Debug, Default)]
pub struct LootLabel {
    pub x: u16,
    pub y: u16,
    pub quality: ItemQuality,
    pub text_hash: u32, // fast hash of OCR text for dedup
}

/// Complete frame state extracted by the vision pipeline.
/// Deliberately kept small (~192 bytes) so clone() is a single memcpy.
/// NO heap allocations — everything is inline/stack.
#[derive(Clone, Debug)]
pub struct FrameState {
    // Survival
    pub hp_pct: u8,
    pub mana_pct: u8,
    pub merc_hp_pct: u8,
    pub merc_alive: bool,

    // Combat
    pub enemy_count: u8,
    pub in_combat: bool,
    pub boss_present: bool,
    pub champion_present: bool,
    pub immune_detected: bool,        // vision saw immunity text on nearest target

    // Nearest enemy screen position (for targeted attacks)
    pub nearest_enemy_x: u16,
    pub nearest_enemy_y: u16,
    pub nearest_enemy_hp_pct: u8,     // 0-100 from health bar vision

    // Location
    pub in_town: bool,
    pub current_act: u8,
    pub motion_magnitude: f32,

    // Character screen position (center of sprite)
    pub char_screen_x: u16,
    pub char_screen_y: u16,

    // Loot
    pub loot_label_count: u8,
    pub loot_labels: [LootLabel; MAX_LOOT_LABELS],

    // Buffs (bitfield: bit 0 = buff slot 0 active, etc.)
    pub active_buffs: u16,

    // Belt potions remaining per column (estimated visually)
    pub belt_columns: [u8; 4],

    // Timing
    pub tick: u64,
    pub tick_phase_ms: u16,
    pub phase_confidence: f32,
    pub capture_time_ns: u64,

    // Image fingerprint
    pub phash: u64,
}

impl Default for FrameState {
    fn default() -> Self {
        Self {
            hp_pct: 100,
            mana_pct: 100,
            merc_hp_pct: 100,
            merc_alive: true,
            enemy_count: 0,
            in_combat: false,
            boss_present: false,
            champion_present: false,
            immune_detected: false,
            nearest_enemy_x: 400,
            nearest_enemy_y: 220,
            nearest_enemy_hp_pct: 0,
            in_town: true,
            current_act: 1,
            motion_magnitude: 0.0,
            char_screen_x: 400,
            char_screen_y: 300,
            loot_label_count: 0,
            loot_labels: Default::default(),
            active_buffs: 0,
            belt_columns: [4, 4, 4, 0],
            tick: 0,
            tick_phase_ms: 0,
            phase_confidence: 0.0,
            capture_time_ns: 0,
            phash: 0,
        }
    }
}

/// One shard in the lock-free buffer.
struct FrameShard {
    state: UnsafeCell<FrameState>,
    /// Sequence number when this shard was last written
    seq: AtomicU64,
    /// 0=idle, 1=writing, 2=complete
    status: AtomicU64,
}

// SAFETY: Single producer writes to one shard at a time (sequencing enforced).
// Readers only access shards with status=COMPLETE.
// Writer never revisits a shard within 15 ticks (640ms at 25Hz),
// far exceeding any reader's hold time.
unsafe impl Sync for FrameShard {}
unsafe impl Send for FrameShard {}

impl FrameShard {
    fn new() -> Self {
        Self {
            state: UnsafeCell::new(FrameState::default()),
            seq: AtomicU64::new(0),
            status: AtomicU64::new(STATUS_IDLE),
        }
    }
}

/// 16-shard lock-free frame buffer.
///
/// # Safety Protocol
///
/// 1. Writer calls `push()` which:
///    - Claims next sequence number atomically
///    - Computes shard index as `seq & 0xF`
///    - Sets shard status to WRITING (readers skip it)
///    - Writes frame state via UnsafeCell
///    - Sets shard status to COMPLETE with seq number
///    - Updates `latest_complete` pointer
///
/// 2. Reader calls `latest()` which:
///    - Reads `latest_complete` pointer
///    - Checks shard status == COMPLETE
///    - Clones the FrameState (~192 byte memcpy)
///    - Verifies status still COMPLETE (ABA protection)
///    - Falls back to scan_latest() on race (extremely rare)
pub struct ShardedFrameBuffer {
    shards: [FrameShard; SHARD_COUNT],
    write_seq: AtomicU64,
    latest_complete: AtomicUsize,
}

impl ShardedFrameBuffer {
    pub fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| FrameShard::new()),
            write_seq: AtomicU64::new(0),
            latest_complete: AtomicUsize::new(0),
        }
    }

    /// Producer: write a new frame state. Wait-free O(1).
    #[inline]
    pub fn push(&self, state: FrameState) {
        let seq = self.write_seq.fetch_add(1, Ordering::AcqRel);
        let idx = (seq as usize) & SHARD_MASK;
        let shard = &self.shards[idx];

        // Mark writing — readers will skip
        shard.status.store(STATUS_WRITING, Ordering::Release);

        // SAFETY: No reader touches a WRITING shard. Single producer.
        unsafe {
            *shard.state.get() = state;
        }

        // Mark complete
        shard.seq.store(seq, Ordering::Release);
        shard.status.store(STATUS_COMPLETE, Ordering::Release);

        // Update latest pointer
        self.latest_complete.store(idx, Ordering::Release);
    }

    /// Consumer: read most recent completed frame. Wait-free O(1) typical, O(16) worst case.
    #[inline]
    pub fn latest(&self) -> Option<FrameState> {
        let idx = self.latest_complete.load(Ordering::Acquire);
        let shard = &self.shards[idx];

        if shard.status.load(Ordering::Acquire) != STATUS_COMPLETE {
            return self.scan_latest();
        }

        let seq_before = shard.seq.load(Ordering::Acquire);

        // SAFETY: shard status is COMPLETE, writer has moved on
        let state = unsafe { (*shard.state.get()).clone() };

        // ABA check: verify shard wasn't recycled during our read
        let seq_after = shard.seq.load(Ordering::Acquire);
        if seq_before == seq_after && shard.status.load(Ordering::Acquire) == STATUS_COMPLETE {
            Some(state)
        } else {
            self.scan_latest()
        }
    }

    /// Consumer: read N most recent frames for trend analysis.
    /// Returns frames in reverse chronological order (newest first).
    pub fn recent(&self, n: usize) -> Vec<FrameState> {
        let current_seq = self.write_seq.load(Ordering::Acquire);
        let count = n.min(SHARD_COUNT);
        let mut frames = Vec::with_capacity(count);

        for i in 0..count {
            let target_seq = match current_seq.checked_sub(i as u64 + 1) {
                Some(s) => s,
                None => break,
            };
            let shard_idx = (target_seq as usize) & SHARD_MASK;
            let shard = &self.shards[shard_idx];

            let status = shard.status.load(Ordering::Acquire);
            let seq = shard.seq.load(Ordering::Acquire);

            if status == STATUS_COMPLETE && seq == target_seq {
                // SAFETY: status COMPLETE + seq match = shard is stable
                let state = unsafe { (*shard.state.get()).clone() };
                // Double-check
                if shard.seq.load(Ordering::Acquire) == target_seq {
                    frames.push(state);
                }
            }
        }

        frames
    }

    /// Fallback scan when fast path races with writer.
    fn scan_latest(&self) -> Option<FrameState> {
        let mut best_seq = 0u64;
        let mut best_idx = None;

        for (i, shard) in self.shards.iter().enumerate() {
            if shard.status.load(Ordering::Acquire) == STATUS_COMPLETE {
                let seq = shard.seq.load(Ordering::Acquire);
                if seq > best_seq {
                    best_seq = seq;
                    best_idx = Some(i);
                }
            }
        }

        best_idx.and_then(|idx| {
            let shard = &self.shards[idx];
            let state = unsafe { (*shard.state.get()).clone() };
            if shard.status.load(Ordering::Acquire) == STATUS_COMPLETE
                && shard.seq.load(Ordering::Acquire) == best_seq
            {
                Some(state)
            } else {
                None
            }
        })
    }

    /// Current write sequence (total frames produced)
    pub fn total_frames(&self) -> u64 {
        self.write_seq.load(Ordering::Relaxed)
    }

    /// Buffer diagnostics
    pub fn stats(&self) -> BufferStats {
        let current_seq = self.write_seq.load(Ordering::Acquire);
        let mut complete = 0u8;
        let mut writing = 0u8;
        let mut idle = 0u8;

        for shard in &self.shards {
            match shard.status.load(Ordering::Relaxed) {
                STATUS_IDLE => idle += 1,
                STATUS_WRITING => writing += 1,
                STATUS_COMPLETE => complete += 1,
                _ => {}
            }
        }

        BufferStats {
            total_frames_written: current_seq,
            shards_complete: complete,
            shards_writing: writing,
            shards_idle: idle,
            current_shard: (current_seq as usize) & SHARD_MASK,
        }
    }
}

impl Default for ShardedFrameBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct BufferStats {
    pub total_frames_written: u64,
    pub shards_complete: u8,
    pub shards_writing: u8,
    pub shards_idle: u8,
    pub current_shard: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn test_single_push_read() {
        let buf = ShardedFrameBuffer::new();

        let mut state = FrameState::default();
        state.hp_pct = 75;
        state.mana_pct = 42;
        state.tick = 1;

        buf.push(state);

        let read = buf.latest().expect("should have a frame");
        assert_eq!(read.hp_pct, 75);
        assert_eq!(read.mana_pct, 42);
        assert_eq!(read.tick, 1);
    }

    #[test]
    fn test_sequential_writes_latest_wins() {
        let buf = ShardedFrameBuffer::new();

        for i in 0..100u64 {
            let mut state = FrameState::default();
            state.hp_pct = (i % 100) as u8;
            state.tick = i;
            buf.push(state);
        }

        let read = buf.latest().expect("should have a frame");
        assert_eq!(read.tick, 99);
        assert_eq!(read.hp_pct, 99);
    }

    #[test]
    fn test_recent_returns_reverse_chronological() {
        let buf = ShardedFrameBuffer::new();

        for i in 0..20u64 {
            let mut state = FrameState::default();
            state.tick = i;
            state.hp_pct = i as u8;
            buf.push(state);
        }

        let recent = buf.recent(5);
        assert!(recent.len() >= 4, "should get at least 4 recent frames");

        // Should be reverse chronological
        for window in recent.windows(2) {
            assert!(
                window[0].tick > window[1].tick,
                "frames should be newest-first: {} > {}",
                window[0].tick,
                window[1].tick
            );
        }
    }

    #[test]
    fn test_empty_buffer_returns_none() {
        let buf = ShardedFrameBuffer::new();
        assert!(buf.latest().is_none());
    }

    #[test]
    fn test_stats() {
        let buf = ShardedFrameBuffer::new();

        let s0 = buf.stats();
        assert_eq!(s0.total_frames_written, 0);
        assert_eq!(s0.shards_idle, 16);

        for i in 0..5 {
            let mut state = FrameState::default();
            state.tick = i;
            buf.push(state);
        }

        let s1 = buf.stats();
        assert_eq!(s1.total_frames_written, 5);
        assert_eq!(s1.shards_complete, 5);
    }

    #[test]
    fn test_shard_wraparound() {
        let buf = ShardedFrameBuffer::new();

        // Write more than 16 frames — forces shard reuse
        for i in 0..50u64 {
            let mut state = FrameState::default();
            state.tick = i;
            state.hp_pct = (50 + i % 50) as u8;
            buf.push(state);
        }

        let read = buf.latest().expect("should have a frame");
        assert_eq!(read.tick, 49);

        let stats = buf.stats();
        assert_eq!(stats.total_frames_written, 50);
        // All 16 shards should be complete (fully cycled)
        assert_eq!(stats.shards_complete, 16);
    }

    #[test]
    fn test_loot_labels() {
        let buf = ShardedFrameBuffer::new();

        let mut state = FrameState::default();
        state.loot_label_count = 3;
        state.loot_labels[0] = LootLabel {
            x: 100,
            y: 200,
            quality: ItemQuality::Unique,
            text_hash: 0xDEAD,
        };
        state.loot_labels[1] = LootLabel {
            x: 300,
            y: 150,
            quality: ItemQuality::Rune,
            text_hash: 0xBEEF,
        };
        state.loot_labels[2] = LootLabel {
            x: 500,
            y: 400,
            quality: ItemQuality::Rare,
            text_hash: 0xCAFE,
        };

        buf.push(state);

        let read = buf.latest().unwrap();
        assert_eq!(read.loot_label_count, 3);
        assert_eq!(read.loot_labels[0].quality, ItemQuality::Unique);
        assert_eq!(read.loot_labels[1].quality, ItemQuality::Rune);
        assert_eq!(read.loot_labels[2].x, 500);
    }

    #[test]
    fn test_concurrent_producer_consumer() {
        let buf = Arc::new(ShardedFrameBuffer::new());
        let total_frames = 10_000u64;

        // Producer thread: simulate 25Hz capture
        let buf_w = Arc::clone(&buf);
        let producer = thread::spawn(move || {
            for i in 0..total_frames {
                let mut state = FrameState::default();
                state.tick = i;
                state.hp_pct = (i % 100) as u8;
                state.enemy_count = (i % 12) as u8;
                buf_w.push(state);
                // Simulate ~40ms tick at accelerated pace
                thread::sleep(Duration::from_micros(10));
            }
        });

        // Consumer thread: read as fast as possible
        let buf_r = Arc::clone(&buf);
        let consumer = thread::spawn(move || {
            let mut reads = 0u64;
            let mut last_tick = 0u64;
            let mut monotonic_violations = 0u64;
            let start = Instant::now();

            while start.elapsed() < Duration::from_secs(3) {
                if let Some(state) = buf_r.latest() {
                    reads += 1;
                    // Tick should be monotonically increasing (or equal on re-read)
                    if state.tick < last_tick {
                        monotonic_violations += 1;
                    }
                    last_tick = state.tick;
                    // Verify data consistency: hp_pct should match tick % 100
                    assert_eq!(
                        state.hp_pct,
                        (state.tick % 100) as u8,
                        "data corruption at tick {}",
                        state.tick
                    );
                }
            }

            (reads, monotonic_violations)
        });

        producer.join().unwrap();
        let (reads, violations) = consumer.join().unwrap();

        println!(
            "Concurrent test: {} reads, {} monotonic violations",
            reads, violations
        );

        // Should have many successful reads
        assert!(reads > 100, "expected many reads, got {}", reads);
        // Zero data corruption (asserted inside consumer)
        // Very few monotonic violations (only possible during shard recycle race)
        assert!(
            violations < 5,
            "too many monotonic violations: {}",
            violations
        );
    }

    #[test]
    fn test_multi_consumer() {
        let buf = Arc::new(ShardedFrameBuffer::new());
        let total_frames = 5_000u64;

        // Producer
        let buf_w = Arc::clone(&buf);
        let producer = thread::spawn(move || {
            for i in 0..total_frames {
                let mut state = FrameState::default();
                state.tick = i;
                state.hp_pct = (i % 100) as u8;
                buf_w.push(state);
                thread::sleep(Duration::from_micros(5));
            }
        });

        // Multiple consumers
        let mut consumers = Vec::new();
        for consumer_id in 0..4 {
            let buf_r = Arc::clone(&buf);
            consumers.push(thread::spawn(move || {
                let mut reads = 0u64;
                let start = Instant::now();

                while start.elapsed() < Duration::from_secs(2) {
                    if let Some(state) = buf_r.latest() {
                        reads += 1;
                        assert_eq!(
                            state.hp_pct,
                            (state.tick % 100) as u8,
                            "consumer {} saw corruption at tick {}",
                            consumer_id,
                            state.tick
                        );
                    }
                }

                reads
            }));
        }

        producer.join().unwrap();
        let total_reads: u64 = consumers
            .into_iter()
            .map(|c| c.join().unwrap())
            .sum();

        println!("Multi-consumer test: {} total reads across 4 consumers", total_reads);
        assert!(total_reads > 400, "expected many reads across consumers");
    }

    #[test]
    fn test_frame_state_size() {
        // Ensure FrameState stays small for fast memcpy
        let size = std::mem::size_of::<FrameState>();
        println!("FrameState size: {} bytes", size);
        assert!(
            size < 256,
            "FrameState too large ({}B) — will slow down clone()",
            size
        );
    }
}
