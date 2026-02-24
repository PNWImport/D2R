//! Syscall Cadence Controller
//!
//! Adds entropy to the timing between system calls to prevent
//! ETW (Event Tracing for Windows) fingerprinting.
//!
//! Problem: Automation tools produce regular, predictable syscall
//! patterns. A bot that calls AcquireNextFrame every 40.0ms and
//! SendInput every 280.0ms creates a distinctive frequency signature
//! in ETW traces that's trivially distinguishable from human-driven
//! applications.
//!
//! Solution: Insert micro-delays with gaussian jitter before syscalls,
//! vary the order of non-critical operations, and occasionally inject
//! decoy syscalls that real applications make (NtQueryVolumeInformation,
//! NtReadFile on config, etc.)

use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Categories of syscalls we need to mask
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyscallCategory {
    /// Desktop Duplication acquire/release
    ScreenCapture,
    /// SendInput for keyboard/mouse
    InputDispatch,
    /// File I/O (logging, config reads)
    FileIO,
    /// Timer/sleep calls
    TimerWait,
    /// Memory operations
    Memory,
    /// Decoy — syscalls injected purely for noise
    Decoy,
}

/// Per-category cadence configuration
#[derive(Debug, Clone)]
pub struct CategoryConfig {
    /// Extra microseconds of jitter (gaussian mean)
    pub jitter_mean_us: f64,
    /// Extra microseconds of jitter (gaussian stddev)
    pub jitter_stddev_us: f64,
    /// Floor — minimum extra delay
    pub jitter_floor_us: u64,
    /// Ceiling — maximum extra delay
    pub jitter_ceil_us: u64,
    /// Probability of inserting a decoy syscall before this one
    pub decoy_injection_rate: f32,
}

impl Default for CategoryConfig {
    fn default() -> Self {
        Self {
            jitter_mean_us: 50.0,
            jitter_stddev_us: 25.0,
            jitter_floor_us: 5,
            jitter_ceil_us: 500,
            decoy_injection_rate: 0.0,
        }
    }
}

/// Full cadence controller configuration
#[derive(Debug, Clone)]
pub struct CadenceConfig {
    pub screen_capture: CategoryConfig,
    pub input_dispatch: CategoryConfig,
    pub file_io: CategoryConfig,
    pub timer_wait: CategoryConfig,
    pub memory: CategoryConfig,
    /// Global decoy injection — independent of category
    pub global_decoy_rate: f32,
    /// Decoy syscalls per injection event
    pub decoy_min_count: u8,
    pub decoy_max_count: u8,
}

impl Default for CadenceConfig {
    fn default() -> Self {
        Self {
            screen_capture: CategoryConfig {
                jitter_mean_us: 80.0,
                jitter_stddev_us: 40.0,
                jitter_floor_us: 10,
                jitter_ceil_us: 800,
                decoy_injection_rate: 0.05,
            },
            input_dispatch: CategoryConfig {
                jitter_mean_us: 40.0,
                jitter_stddev_us: 20.0,
                jitter_floor_us: 5,
                jitter_ceil_us: 300,
                decoy_injection_rate: 0.08,
            },
            file_io: CategoryConfig {
                jitter_mean_us: 20.0,
                jitter_stddev_us: 15.0,
                jitter_floor_us: 0,
                jitter_ceil_us: 200,
                decoy_injection_rate: 0.0,
            },
            timer_wait: CategoryConfig {
                jitter_mean_us: 10.0,
                jitter_stddev_us: 8.0,
                jitter_floor_us: 0,
                jitter_ceil_us: 100,
                decoy_injection_rate: 0.0,
            },
            memory: CategoryConfig::default(),
            global_decoy_rate: 0.02,
            decoy_min_count: 1,
            decoy_max_count: 3,
        }
    }
}

/// Tracks cadence statistics per category
#[derive(Debug, Default)]
struct CategoryStats {
    call_count: AtomicU64,
    total_jitter_us: AtomicU64,
    decoy_injections: AtomicU64,
}

/// Decoy syscall types (harmless calls real apps make)
#[derive(Debug, Clone, Copy)]
pub enum DecoyType {
    /// Read a random registry key (Chrome does this constantly)
    RegistryQuery,
    /// Query system time with high precision
    QueryPerformanceCounter,
    /// Check available memory
    GlobalMemoryStatus,
    /// Stat a file (Chrome's profile directory)
    FileAttributeCheck,
    /// Read environment variable
    GetEnvironmentVariable,
    /// Query thread times
    QueryThreadTimes,
}

impl DecoyType {
    fn all() -> &'static [DecoyType] {
        &[
            DecoyType::RegistryQuery,
            DecoyType::QueryPerformanceCounter,
            DecoyType::GlobalMemoryStatus,
            DecoyType::FileAttributeCheck,
            DecoyType::GetEnvironmentVariable,
            DecoyType::QueryThreadTimes,
        ]
    }
}

pub struct SyscallCadence {
    config: CadenceConfig,
    rng: StdRng,
    distributions: CadenceDistributions,
    stats: CadenceStats,
    _last_decoy_time: Instant,
}

struct CadenceDistributions {
    screen_capture: Normal<f64>,
    input_dispatch: Normal<f64>,
    file_io: Normal<f64>,
    timer_wait: Normal<f64>,
    memory: Normal<f64>,
}

#[derive(Default)]
struct CadenceStats {
    screen_capture: CategoryStats,
    input_dispatch: CategoryStats,
    file_io: CategoryStats,
    timer_wait: CategoryStats,
    memory: CategoryStats,
    total_decoys: AtomicU64,
}

fn make_dist(cfg: &CategoryConfig) -> Normal<f64> {
    Normal::new(cfg.jitter_mean_us, cfg.jitter_stddev_us)
        .unwrap_or_else(|_| Normal::new(50.0, 25.0).unwrap())
}

impl SyscallCadence {
    pub fn new(config: CadenceConfig) -> Self {
        let distributions = CadenceDistributions {
            screen_capture: make_dist(&config.screen_capture),
            input_dispatch: make_dist(&config.input_dispatch),
            file_io: make_dist(&config.file_io),
            timer_wait: make_dist(&config.timer_wait),
            memory: make_dist(&config.memory),
        };

        Self {
            config,
            rng: StdRng::from_entropy(),
            distributions,
            stats: CadenceStats::default(),
            _last_decoy_time: Instant::now(),
        }
    }

    /// Call before making a syscall. Returns:
    /// - Pre-call jitter duration to sleep
    /// - Number of decoy syscalls to inject
    /// - Which decoy types to use
    pub fn pre_syscall(&mut self, category: SyscallCategory) -> SyscallPrep {
        // Extract config values upfront to avoid borrow conflicts
        let (jitter_floor, jitter_ceil, decoy_injection_rate) = match category {
            SyscallCategory::ScreenCapture => (
                self.config.screen_capture.jitter_floor_us,
                self.config.screen_capture.jitter_ceil_us,
                self.config.screen_capture.decoy_injection_rate,
            ),
            SyscallCategory::InputDispatch => (
                self.config.input_dispatch.jitter_floor_us,
                self.config.input_dispatch.jitter_ceil_us,
                self.config.input_dispatch.decoy_injection_rate,
            ),
            SyscallCategory::FileIO => (
                self.config.file_io.jitter_floor_us,
                self.config.file_io.jitter_ceil_us,
                self.config.file_io.decoy_injection_rate,
            ),
            SyscallCategory::TimerWait => (
                self.config.timer_wait.jitter_floor_us,
                self.config.timer_wait.jitter_ceil_us,
                self.config.timer_wait.decoy_injection_rate,
            ),
            SyscallCategory::Memory | SyscallCategory::Decoy => (
                self.config.memory.jitter_floor_us,
                self.config.memory.jitter_ceil_us,
                self.config.memory.decoy_injection_rate,
            ),
        };

        let global_decoy_rate = self.config.global_decoy_rate;
        let decoy_min = self.config.decoy_min_count;
        let decoy_max = self.config.decoy_max_count;

        // Sample jitter from the right distribution
        let raw_jitter = match category {
            SyscallCategory::ScreenCapture => {
                self.distributions.screen_capture.sample(&mut self.rng)
            }
            SyscallCategory::InputDispatch => {
                self.distributions.input_dispatch.sample(&mut self.rng)
            }
            SyscallCategory::FileIO => self.distributions.file_io.sample(&mut self.rng),
            SyscallCategory::TimerWait => self.distributions.timer_wait.sample(&mut self.rng),
            SyscallCategory::Memory | SyscallCategory::Decoy => {
                self.distributions.memory.sample(&mut self.rng)
            }
        };

        let clamped = raw_jitter.clamp(jitter_floor as f64, jitter_ceil as f64);
        let jitter = Duration::from_micros(clamped as u64);

        // Update stats
        let stat = match category {
            SyscallCategory::ScreenCapture => &self.stats.screen_capture,
            SyscallCategory::InputDispatch => &self.stats.input_dispatch,
            SyscallCategory::FileIO => &self.stats.file_io,
            SyscallCategory::TimerWait => &self.stats.timer_wait,
            SyscallCategory::Memory | SyscallCategory::Decoy => &self.stats.memory,
        };
        stat.call_count.fetch_add(1, Ordering::Relaxed);
        stat.total_jitter_us
            .fetch_add(clamped as u64, Ordering::Relaxed);

        // Decide on decoy injection
        let mut decoys = Vec::new();

        if self.rng.gen::<f32>() < decoy_injection_rate {
            let count = self.rng.gen_range(decoy_min..=decoy_max);
            for _ in 0..count {
                let all = DecoyType::all();
                let decoy = all[self.rng.gen_range(0..all.len())];
                decoys.push(decoy);
            }
            stat.decoy_injections.fetch_add(1, Ordering::Relaxed);
        }

        if self.rng.gen::<f32>() < global_decoy_rate {
            let all = DecoyType::all();
            decoys.push(all[self.rng.gen_range(0..all.len())]);
            self.stats.total_decoys.fetch_add(1, Ordering::Relaxed);
        }

        SyscallPrep {
            jitter,
            decoys,
            category,
        }
    }

    /// Execute decoy syscalls. Call this between the jitter sleep and the real syscall.
    pub fn execute_decoys(&mut self, prep: &SyscallPrep) {
        for decoy in &prep.decoys {
            self.execute_single_decoy(*decoy);
            // Tiny random pause between decoys
            let pause_us = self.rng.gen_range(5..50);
            std::thread::sleep(Duration::from_micros(pause_us));
        }
    }

    fn execute_single_decoy(&mut self, decoy: DecoyType) {
        match decoy {
            DecoyType::QueryPerformanceCounter => {
                #[cfg(windows)]
                unsafe {
                    let mut counter: i64 = 0;
                    let _ = windows::Win32::System::Performance::QueryPerformanceCounter(&mut counter);
                }
                #[cfg(not(windows))]
                {
                    let _ = Instant::now();
                }
            }
            DecoyType::GlobalMemoryStatus => {
                #[cfg(windows)]
                unsafe {
                    let mut mem =
                        windows::Win32::System::SystemInformation::MEMORYSTATUSEX::default();
                    mem.dwLength = std::mem::size_of::<
                        windows::Win32::System::SystemInformation::MEMORYSTATUSEX,
                    >() as u32;
                    let _ =
                        windows::Win32::System::SystemInformation::GlobalMemoryStatusEx(&mut mem);
                }
                #[cfg(not(windows))]
                {
                    let _ = std::fs::read_to_string("/proc/meminfo")
                        .map(|s| s.lines().next().map(|l| l.len()));
                }
            }
            DecoyType::FileAttributeCheck => {
                #[cfg(windows)]
                unsafe {
                    use windows::core::w;
                    let _ = windows::Win32::Storage::FileSystem::GetFileAttributesW(w!(
                        "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"
                    ));
                }
                #[cfg(not(windows))]
                {
                    let _ = std::fs::metadata("/tmp/.chrome_lock");
                }
            }
            DecoyType::GetEnvironmentVariable => {
                let vars = [
                    "TEMP",
                    "PATH",
                    "USERPROFILE",
                    "APPDATA",
                    "LOCALAPPDATA",
                    "CHROME_PATH",
                ];
                let var = vars[self.rng.gen_range(0..vars.len())];
                let _ = std::env::var(var);
            }
            DecoyType::QueryThreadTimes => {
                #[cfg(windows)]
                unsafe {
                    let thread = windows::Win32::System::Threading::GetCurrentThread();
                    let mut ct = windows::Win32::Foundation::FILETIME::default();
                    let mut et = windows::Win32::Foundation::FILETIME::default();
                    let mut kt = windows::Win32::Foundation::FILETIME::default();
                    let mut ut = windows::Win32::Foundation::FILETIME::default();
                    let _ = windows::Win32::System::Threading::GetThreadTimes(
                        thread, &mut ct, &mut et, &mut kt, &mut ut,
                    );
                }
                #[cfg(not(windows))]
                {
                    let _ = std::time::SystemTime::now();
                }
            }
            DecoyType::RegistryQuery => {
                #[cfg(windows)]
                unsafe {
                    use windows::Win32::System::Registry::*;
                    let mut hkey = HKEY::default();
                    let _ = RegOpenKeyExW(
                        HKEY_LOCAL_MACHINE,
                        windows::core::w!("SOFTWARE\\Google\\Chrome"),
                        0,
                        KEY_READ,
                        &mut hkey,
                    );
                    if !hkey.is_invalid() {
                        let _ = RegCloseKey(hkey);
                    }
                }
                #[cfg(not(windows))]
                {
                    let _ = std::fs::metadata("/etc/hostname");
                }
            }
        }
        self.stats.total_decoys.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> CadenceReport {
        let cat_stats = |s: &CategoryStats| -> CadenceCategoryReport {
            let count = s.call_count.load(Ordering::Relaxed);
            let total_jitter = s.total_jitter_us.load(Ordering::Relaxed);
            CadenceCategoryReport {
                call_count: count,
                avg_jitter_us: if count > 0 { total_jitter / count } else { 0 },
                decoy_injections: s.decoy_injections.load(Ordering::Relaxed),
            }
        };

        CadenceReport {
            screen_capture: cat_stats(&self.stats.screen_capture),
            input_dispatch: cat_stats(&self.stats.input_dispatch),
            file_io: cat_stats(&self.stats.file_io),
            timer_wait: cat_stats(&self.stats.timer_wait),
            memory: cat_stats(&self.stats.memory),
            total_decoys_executed: self.stats.total_decoys.load(Ordering::Relaxed),
        }
    }
}

/// Preparation result — tells the caller what to do before the syscall
#[derive(Debug)]
pub struct SyscallPrep {
    /// Sleep this long before the syscall
    pub jitter: Duration,
    /// Execute these decoy syscalls first
    pub decoys: Vec<DecoyType>,
    pub category: SyscallCategory,
}

#[derive(Debug, Clone)]
pub struct CadenceReport {
    pub screen_capture: CadenceCategoryReport,
    pub input_dispatch: CadenceCategoryReport,
    pub file_io: CadenceCategoryReport,
    pub timer_wait: CadenceCategoryReport,
    pub memory: CadenceCategoryReport,
    pub total_decoys_executed: u64,
}

#[derive(Debug, Clone)]
pub struct CadenceCategoryReport {
    pub call_count: u64,
    pub avg_jitter_us: u64,
    pub decoy_injections: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CadenceConfig::default();
        assert!(config.screen_capture.jitter_mean_us > 0.0);
        assert!(config.input_dispatch.decoy_injection_rate > 0.0);
        assert!(config.global_decoy_rate > 0.0);
    }

    #[test]
    fn test_jitter_in_range() {
        let mut cadence = SyscallCadence::new(CadenceConfig::default());

        for _ in 0..1000 {
            let prep = cadence.pre_syscall(SyscallCategory::ScreenCapture);
            let us = prep.jitter.as_micros() as u64;
            assert!(
                us >= 10 && us <= 800,
                "jitter {}µs outside screen_capture range [10, 800]",
                us
            );
        }
    }

    #[test]
    fn test_jitter_distribution() {
        let mut cadence = SyscallCadence::new(CadenceConfig::default());
        let mut jitters = Vec::with_capacity(10000);

        for _ in 0..10000 {
            let prep = cadence.pre_syscall(SyscallCategory::InputDispatch);
            jitters.push(prep.jitter.as_micros() as f64);
        }

        let mean = jitters.iter().sum::<f64>() / jitters.len() as f64;
        let variance =
            jitters.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / jitters.len() as f64;
        let stddev = variance.sqrt();

        println!(
            "Input dispatch jitter: mean={:.1}µs stddev={:.1}µs",
            mean, stddev
        );

        // Mean should be near config (40µs)
        assert!(
            mean > 25.0 && mean < 60.0,
            "mean {:.1}µs outside expected range",
            mean
        );
        // Should have variance
        assert!(stddev > 5.0, "stddev too low: {:.1}µs", stddev);
    }

    #[test]
    fn test_decoy_injection_occurs() {
        let config = CadenceConfig {
            screen_capture: CategoryConfig {
                decoy_injection_rate: 0.5, // High rate for testing
                ..Default::default()
            },
            global_decoy_rate: 0.0,
            ..Default::default()
        };
        let mut cadence = SyscallCadence::new(config);

        let mut decoy_count = 0;
        for _ in 0..100 {
            let prep = cadence.pre_syscall(SyscallCategory::ScreenCapture);
            if !prep.decoys.is_empty() {
                decoy_count += 1;
                cadence.execute_decoys(&prep);
            }
        }

        assert!(
            decoy_count > 20,
            "expected ~50 decoy injections at 50% rate, got {}",
            decoy_count
        );
    }

    #[test]
    fn test_decoy_execution_doesnt_crash() {
        let mut cadence = SyscallCadence::new(CadenceConfig::default());

        // Execute every decoy type directly
        for decoy in DecoyType::all() {
            cadence.execute_single_decoy(*decoy);
        }

        let stats = cadence.get_stats();
        assert!(stats.total_decoys_executed > 0);
    }

    #[test]
    fn test_stats_tracking() {
        let mut cadence = SyscallCadence::new(CadenceConfig::default());

        for _ in 0..50 {
            let prep = cadence.pre_syscall(SyscallCategory::ScreenCapture);
            cadence.execute_decoys(&prep);
        }
        for _ in 0..30 {
            let prep = cadence.pre_syscall(SyscallCategory::InputDispatch);
            cadence.execute_decoys(&prep);
        }

        let report = cadence.get_stats();
        assert_eq!(report.screen_capture.call_count, 50);
        assert_eq!(report.input_dispatch.call_count, 30);
        assert!(report.screen_capture.avg_jitter_us > 0);
    }

    #[test]
    fn test_different_categories_different_jitter() {
        let mut cadence = SyscallCadence::new(CadenceConfig::default());

        let mut capture_jitters = Vec::new();
        let mut input_jitters = Vec::new();

        for _ in 0..5000 {
            capture_jitters.push(
                cadence
                    .pre_syscall(SyscallCategory::ScreenCapture)
                    .jitter
                    .as_micros() as f64,
            );
            input_jitters.push(
                cadence
                    .pre_syscall(SyscallCategory::InputDispatch)
                    .jitter
                    .as_micros() as f64,
            );
        }

        let cap_mean = capture_jitters.iter().sum::<f64>() / capture_jitters.len() as f64;
        let inp_mean = input_jitters.iter().sum::<f64>() / input_jitters.len() as f64;

        // Screen capture should have higher jitter than input (80µs vs 40µs mean)
        assert!(
            cap_mean > inp_mean,
            "capture mean ({:.1}) should exceed input mean ({:.1})",
            cap_mean,
            inp_mean
        );
    }

    #[test]
    fn test_no_decoys_when_rate_zero() {
        let config = CadenceConfig {
            screen_capture: CategoryConfig {
                decoy_injection_rate: 0.0,
                ..Default::default()
            },
            global_decoy_rate: 0.0,
            ..Default::default()
        };
        let mut cadence = SyscallCadence::new(config);

        for _ in 0..1000 {
            let prep = cadence.pre_syscall(SyscallCategory::ScreenCapture);
            assert!(
                prep.decoys.is_empty(),
                "no decoys should be injected at 0% rate"
            );
        }
    }
}
