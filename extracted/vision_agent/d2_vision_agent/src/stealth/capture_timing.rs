//! Jittered Capture Timing
//!
//! Prevents cadence fingerprinting by varying capture intervals.
//! Instead of fixed 25 Hz (40ms), uses gaussian jitter + burst mode
//! + occasional skips to mimic real screen recording applications.
//!
//! Key insight: A constant 25.000 Hz capture rate is a statistical
//! anomaly. Real applications (OBS, Discord, WebRTC) show jitter
//! from GC pauses, vsync drift, OS scheduling, and user interaction.
//!
//! # Burst Mode
//!
//! Instead of holding a Desktop Duplication handle permanently,
//! we acquire → capture N frames → release → pause → repeat.
//! This mimics screen recording apps that start/stop, and prevents
//! the handle table from showing a persistent capture handle.

use rand::prelude::*;
use rand_distr::{Distribution, Normal};
use std::time::{Duration, Instant};

/// Capture timing mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaptureMode {
    /// Continuous capture with jittered intervals
    Continuous,
    /// Burst capture: acquire handle → N frames → release → pause
    Burst,
}

/// Configuration for capture timing
#[derive(Debug, Clone)]
pub struct CaptureTimingConfig {
    /// Target FPS (center of jitter distribution)
    pub target_fps: f32,
    /// Stddev of frame interval in ms
    pub interval_jitter_ms: f32,
    /// Min frame interval (prevents CPU spinning)
    pub min_interval_ms: f32,
    /// Max frame interval (prevents stale data)
    pub max_interval_ms: f32,
    /// Probability of skipping a frame (simulates blink/distraction)
    pub skip_rate: f32,
    /// Duration of a skip in ms range
    pub skip_min_ms: u64,
    pub skip_max_ms: u64,
    /// Probability of a long pause per frame (alt-tab, phone check)
    pub long_pause_rate: f32,
    pub long_pause_min_ms: u64,
    pub long_pause_max_ms: u64,
    /// Burst mode settings
    pub mode: CaptureMode,
    pub burst_min_frames: u32,
    pub burst_max_frames: u32,
    pub burst_pause_min_ms: u64,
    pub burst_pause_max_ms: u64,
}

impl Default for CaptureTimingConfig {
    fn default() -> Self {
        Self {
            target_fps: 25.0,
            interval_jitter_ms: 6.0,
            min_interval_ms: 28.0,
            max_interval_ms: 55.0,
            skip_rate: 0.02,
            skip_min_ms: 80,
            skip_max_ms: 200,
            long_pause_rate: 0.001,
            long_pause_min_ms: 500,
            long_pause_max_ms: 3000,
            mode: CaptureMode::Burst,
            burst_min_frames: 50,  // 2 seconds at 25fps
            burst_max_frames: 300, // 12 seconds at 25fps
            burst_pause_min_ms: 150,
            burst_pause_max_ms: 600,
        }
    }
}

/// Capture timing controller
pub struct CaptureTiming {
    config: CaptureTimingConfig,
    rng: ThreadRng,
    interval_dist: Normal<f64>,

    // Burst mode state
    frames_in_burst: u32,
    burst_target: u32,
    in_burst: bool,

    // Statistics
    total_frames: u64,
    total_skips: u64,
    total_pauses: u64,
    total_bursts: u64,
    last_frame_time: Instant,
}

/// What the capture loop should do next
#[derive(Debug, Clone)]
pub enum CaptureAction {
    /// Capture a frame, then wait this duration
    CaptureAndWait(Duration),
    /// Skip this frame, wait this duration
    Skip(Duration),
    /// Long pause (alt-tab simulation)
    LongPause(Duration),
    /// End current burst: release handle, wait, then re-acquire
    EndBurst(Duration),
    /// Start new burst: acquire handle
    StartBurst,
}

impl CaptureTiming {
    pub fn new(config: CaptureTimingConfig) -> Self {
        let target_interval_ms = 1000.0 / config.target_fps as f64;
        let interval_dist = Normal::new(target_interval_ms, config.interval_jitter_ms as f64)
            .unwrap_or_else(|_| Normal::new(40.0, 6.0).unwrap());

        let mut rng = thread_rng();
        let burst_target = if config.mode == CaptureMode::Burst {
            rng.gen_range(config.burst_min_frames..=config.burst_max_frames)
        } else {
            u32::MAX
        };

        Self {
            config,
            rng,
            interval_dist,
            frames_in_burst: 0,
            burst_target,
            in_burst: false,
            total_frames: 0,
            total_skips: 0,
            total_pauses: 0,
            total_bursts: 0,
            last_frame_time: Instant::now(),
        }
    }

    /// Get the next capture action. Call this in the main capture loop.
    pub fn next_action(&mut self) -> CaptureAction {
        // Burst mode: check if we need to start or end a burst
        if self.config.mode == CaptureMode::Burst {
            if !self.in_burst {
                self.in_burst = true;
                self.frames_in_burst = 0;
                self.burst_target = self.rng.gen_range(
                    self.config.burst_min_frames..=self.config.burst_max_frames,
                );
                self.total_bursts += 1;
                return CaptureAction::StartBurst;
            }

            if self.frames_in_burst >= self.burst_target {
                self.in_burst = false;
                let pause = self.rng.gen_range(
                    self.config.burst_pause_min_ms..=self.config.burst_pause_max_ms,
                );
                return CaptureAction::EndBurst(Duration::from_millis(pause));
            }
        }

        // Long pause check (very rare: ~0.1% per frame = ~9/hour at 25Hz)
        if self.rng.gen::<f32>() < self.config.long_pause_rate {
            self.total_pauses += 1;
            let pause = self.rng.gen_range(
                self.config.long_pause_min_ms..=self.config.long_pause_max_ms,
            );
            return CaptureAction::LongPause(Duration::from_millis(pause));
        }

        // Skip check (2% default)
        if self.rng.gen::<f32>() < self.config.skip_rate {
            self.total_skips += 1;
            let skip = self
                .rng
                .gen_range(self.config.skip_min_ms..=self.config.skip_max_ms);
            return CaptureAction::Skip(Duration::from_millis(skip));
        }

        // Normal capture with jittered interval
        let interval_ms = self
            .interval_dist
            .sample(&mut self.rng)
            .clamp(self.config.min_interval_ms as f64, self.config.max_interval_ms as f64);

        self.total_frames += 1;
        self.frames_in_burst += 1;
        self.last_frame_time = Instant::now();

        CaptureAction::CaptureAndWait(Duration::from_millis(interval_ms as u64))
    }

    /// Compensate for actual frame processing time.
    /// Returns adjusted sleep duration.
    pub fn compensated_wait(&self, action_wait: Duration, processing_time: Duration) -> Duration {
        action_wait.saturating_sub(processing_time)
    }

    pub fn stats(&self) -> CaptureTimingStats {
        CaptureTimingStats {
            total_frames: self.total_frames,
            total_skips: self.total_skips,
            total_pauses: self.total_pauses,
            total_bursts: self.total_bursts,
            frames_in_current_burst: self.frames_in_burst,
            current_burst_target: self.burst_target,
            in_burst: self.in_burst,
        }
    }

    pub fn update_config(&mut self, config: CaptureTimingConfig) {
        let target_interval_ms = 1000.0 / config.target_fps as f64;
        self.interval_dist = Normal::new(target_interval_ms, config.interval_jitter_ms as f64)
            .unwrap_or_else(|_| Normal::new(40.0, 6.0).unwrap());
        self.config = config;
    }
}

#[derive(Debug, Clone)]
pub struct CaptureTimingStats {
    pub total_frames: u64,
    pub total_skips: u64,
    pub total_pauses: u64,
    pub total_bursts: u64,
    pub frames_in_current_burst: u32,
    pub current_burst_target: u32,
    pub in_burst: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CaptureTimingConfig::default();
        assert_eq!(config.target_fps, 25.0);
        assert_eq!(config.mode, CaptureMode::Burst);
        assert!(config.skip_rate > 0.0);
        assert!(config.long_pause_rate > 0.0);
    }

    #[test]
    fn test_continuous_mode_produces_captures() {
        let config = CaptureTimingConfig {
            mode: CaptureMode::Continuous,
            skip_rate: 0.0,       // disable skips for deterministic test
            long_pause_rate: 0.0, // disable pauses
            ..Default::default()
        };
        let mut timing = CaptureTiming::new(config);

        let mut captures = 0;
        for _ in 0..1000 {
            match timing.next_action() {
                CaptureAction::CaptureAndWait(d) => {
                    captures += 1;
                    // Interval should be in valid range
                    assert!(d.as_millis() >= 28, "interval too short: {:?}", d);
                    assert!(d.as_millis() <= 55, "interval too long: {:?}", d);
                }
                _ => {}
            }
        }

        assert_eq!(captures, 1000, "all actions should be captures in continuous mode with no skips");
    }

    #[test]
    fn test_burst_mode_starts_and_ends() {
        let config = CaptureTimingConfig {
            mode: CaptureMode::Burst,
            burst_min_frames: 10,
            burst_max_frames: 20,
            skip_rate: 0.0,
            long_pause_rate: 0.0,
            ..Default::default()
        };
        let mut timing = CaptureTiming::new(config);

        let mut saw_start = false;
        let mut saw_end = false;
        let mut saw_capture = false;

        for _ in 0..100 {
            match timing.next_action() {
                CaptureAction::StartBurst => saw_start = true,
                CaptureAction::EndBurst(_) => saw_end = true,
                CaptureAction::CaptureAndWait(_) => saw_capture = true,
                _ => {}
            }
        }

        assert!(saw_start, "should see burst starts");
        assert!(saw_end, "should see burst ends");
        assert!(saw_capture, "should see captures");
    }

    #[test]
    fn test_interval_distribution() {
        let config = CaptureTimingConfig {
            mode: CaptureMode::Continuous,
            skip_rate: 0.0,
            long_pause_rate: 0.0,
            ..Default::default()
        };
        let mut timing = CaptureTiming::new(config);
        let mut intervals_ms = Vec::with_capacity(10000);

        for _ in 0..10000 {
            if let CaptureAction::CaptureAndWait(d) = timing.next_action() {
                intervals_ms.push(d.as_millis() as f64);
            }
        }

        let mean = intervals_ms.iter().sum::<f64>() / intervals_ms.len() as f64;
        let variance = intervals_ms
            .iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>()
            / intervals_ms.len() as f64;
        let stddev = variance.sqrt();

        // Mean should be near 40ms (1000/25fps)
        assert!(
            mean > 37.0 && mean < 43.0,
            "mean interval {:.1}ms outside expected range",
            mean
        );
        // Should have meaningful jitter
        assert!(
            stddev > 3.0,
            "stddev {:.1}ms too low — not enough jitter",
            stddev
        );

        println!(
            "Capture interval: mean={:.1}ms stddev={:.1}ms (target: 40ms ± 6ms)",
            mean, stddev
        );
    }

    #[test]
    fn test_skip_rate() {
        let config = CaptureTimingConfig {
            mode: CaptureMode::Continuous,
            skip_rate: 0.10, // 10% for faster testing
            long_pause_rate: 0.0,
            ..Default::default()
        };
        let mut timing = CaptureTiming::new(config);

        let mut skips = 0;
        let total = 10000;
        for _ in 0..total {
            if matches!(timing.next_action(), CaptureAction::Skip(_)) {
                skips += 1;
            }
        }

        let skip_pct = skips as f64 / total as f64;
        assert!(
            skip_pct > 0.05 && skip_pct < 0.20,
            "skip rate {:.1}% outside expected range (target 10%)",
            skip_pct * 100.0
        );
    }

    #[test]
    fn test_burst_length_varies() {
        let config = CaptureTimingConfig {
            mode: CaptureMode::Burst,
            burst_min_frames: 20,
            burst_max_frames: 80,
            skip_rate: 0.0,
            long_pause_rate: 0.0,
            ..Default::default()
        };
        let mut timing = CaptureTiming::new(config);

        let mut burst_lengths = Vec::new();
        let mut current_burst = 0u32;

        for _ in 0..5000 {
            match timing.next_action() {
                CaptureAction::StartBurst => {
                    if current_burst > 0 {
                        burst_lengths.push(current_burst);
                    }
                    current_burst = 0;
                }
                CaptureAction::CaptureAndWait(_) => current_burst += 1,
                CaptureAction::EndBurst(_) => {
                    burst_lengths.push(current_burst);
                    current_burst = 0;
                }
                _ => {}
            }
        }

        assert!(
            burst_lengths.len() >= 3,
            "should have multiple bursts, got {}",
            burst_lengths.len()
        );

        // Check burst lengths vary
        let unique_lengths: std::collections::HashSet<_> = burst_lengths.iter().collect();
        assert!(
            unique_lengths.len() > 1,
            "burst lengths should vary, got {:?}",
            burst_lengths
        );

        // All bursts should be in valid range
        for len in &burst_lengths {
            assert!(
                *len >= 20 && *len <= 80,
                "burst length {} outside range [20,80]",
                len
            );
        }

        println!("Burst lengths: {:?}", burst_lengths);
    }

    #[test]
    fn test_compensated_wait() {
        let timing = CaptureTiming::new(CaptureTimingConfig::default());

        // If action says wait 40ms and processing took 15ms, sleep 25ms
        let wait = timing.compensated_wait(
            Duration::from_millis(40),
            Duration::from_millis(15),
        );
        assert_eq!(wait, Duration::from_millis(25));

        // If processing took longer than wait, return zero (don't go negative)
        let wait = timing.compensated_wait(
            Duration::from_millis(40),
            Duration::from_millis(50),
        );
        assert_eq!(wait, Duration::ZERO);
    }

    #[test]
    fn test_stats_tracking() {
        let config = CaptureTimingConfig {
            mode: CaptureMode::Burst,
            burst_min_frames: 5,
            burst_max_frames: 10,
            skip_rate: 0.0,
            long_pause_rate: 0.0,
            ..Default::default()
        };
        let mut timing = CaptureTiming::new(config);

        for _ in 0..100 {
            timing.next_action();
        }

        let stats = timing.stats();
        assert!(stats.total_frames > 0);
        assert!(stats.total_bursts > 0);
    }
}
