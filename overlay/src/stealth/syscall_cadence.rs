#![allow(dead_code)]
//! Syscall Cadence Controller for Map Helper
//!
//! Adds entropy to the timing between ReadProcessMemory calls to prevent
//! ETW (Event Tracing for Windows) fingerprinting.
//!
//! The map helper makes RPM calls at regular intervals — this creates a
//! distinctive frequency signature in ETW traces. This module:
//!   1. Inserts gaussian-distributed micro-delays before RPM calls
//!   2. Injects decoy syscalls that real Chrome processes make
//!   3. Varies timing per-category (memory reads vs file I/O)

use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Categories of syscalls we need to mask
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallCategory {
    /// ReadProcessMemory calls (the hot path)
    Memory,
    /// File I/O (config reads, logging)
    FileIO,
    /// Decoy — injected purely for noise
    Decoy,
}

/// Per-category cadence configuration
#[derive(Debug, Clone)]
pub struct CategoryConfig {
    /// Gaussian mean for jitter (microseconds)
    pub jitter_mean_us: f64,
    /// Gaussian stddev
    pub jitter_stddev_us: f64,
    /// Minimum extra delay
    pub jitter_floor_us: u64,
    /// Maximum extra delay
    pub jitter_ceil_us: u64,
    /// Probability of inserting decoy syscalls before this call
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

/// Full cadence controller config
#[derive(Debug, Clone)]
pub struct CadenceConfig {
    pub memory: CategoryConfig,
    pub file_io: CategoryConfig,
    /// Global decoy injection rate (independent of category)
    pub global_decoy_rate: f32,
    pub decoy_min_count: u8,
    pub decoy_max_count: u8,
}

impl Default for CadenceConfig {
    fn default() -> Self {
        Self {
            memory: CategoryConfig {
                jitter_mean_us: 60.0,
                jitter_stddev_us: 30.0,
                jitter_floor_us: 10,
                jitter_ceil_us: 600,
                decoy_injection_rate: 0.08,
            },
            file_io: CategoryConfig {
                jitter_mean_us: 20.0,
                jitter_stddev_us: 15.0,
                jitter_floor_us: 0,
                jitter_ceil_us: 200,
                decoy_injection_rate: 0.0,
            },
            global_decoy_rate: 0.03,
            decoy_min_count: 1,
            decoy_max_count: 3,
        }
    }
}

/// Decoy syscall types (harmless calls real Chrome processes make)
#[derive(Debug, Clone, Copy)]
pub enum DecoyType {
    /// Query system time with high precision
    QueryPerformanceCounter,
    /// Check available memory
    GlobalMemoryStatus,
    /// Stat a file (Chrome profile path)
    FileAttributeCheck,
    /// Read environment variable
    GetEnvironmentVariable,
    /// Query thread times
    QueryThreadTimes,
    /// Query registry (Chrome does this constantly)
    RegistryQuery,
}

impl DecoyType {
    fn all() -> &'static [DecoyType] {
        &[
            DecoyType::QueryPerformanceCounter,
            DecoyType::GlobalMemoryStatus,
            DecoyType::FileAttributeCheck,
            DecoyType::GetEnvironmentVariable,
            DecoyType::QueryThreadTimes,
            DecoyType::RegistryQuery,
        ]
    }
}

struct CategoryStats {
    call_count: AtomicU64,
    total_jitter_us: AtomicU64,
    decoy_injections: AtomicU64,
}

impl Default for CategoryStats {
    fn default() -> Self {
        Self {
            call_count: AtomicU64::new(0),
            total_jitter_us: AtomicU64::new(0),
            decoy_injections: AtomicU64::new(0),
        }
    }
}

pub struct SyscallCadence {
    config: CadenceConfig,
    rng: StdRng,
    memory_dist: Normal<f64>,
    file_io_dist: Normal<f64>,
    memory_stats: CategoryStats,
    file_io_stats: CategoryStats,
    total_decoys: AtomicU64,
}

fn make_dist(cfg: &CategoryConfig) -> Normal<f64> {
    Normal::new(cfg.jitter_mean_us, cfg.jitter_stddev_us)
        .unwrap_or_else(|_| Normal::new(50.0, 25.0).unwrap())
}

impl SyscallCadence {
    pub fn new(config: CadenceConfig) -> Self {
        let memory_dist = make_dist(&config.memory);
        let file_io_dist = make_dist(&config.file_io);

        Self {
            config,
            rng: StdRng::from_entropy(),
            memory_dist,
            file_io_dist,
            memory_stats: CategoryStats::default(),
            file_io_stats: CategoryStats::default(),
            total_decoys: AtomicU64::new(0),
        }
    }

    /// Call before making a syscall. Returns jitter duration and decoy list.
    pub fn pre_syscall(&mut self, category: SyscallCategory) -> SyscallPrep {
        let (cfg, dist, stat) = match category {
            SyscallCategory::Memory | SyscallCategory::Decoy => (
                &self.config.memory,
                &self.memory_dist,
                &self.memory_stats,
            ),
            SyscallCategory::FileIO => (
                &self.config.file_io,
                &self.file_io_dist,
                &self.file_io_stats,
            ),
        };

        let jitter_floor = cfg.jitter_floor_us;
        let jitter_ceil = cfg.jitter_ceil_us;
        let decoy_rate = cfg.decoy_injection_rate;

        // Sample jitter from gaussian distribution
        let raw_jitter = dist.sample(&mut self.rng);
        let clamped = raw_jitter.clamp(jitter_floor as f64, jitter_ceil as f64);
        let jitter = Duration::from_micros(clamped as u64);

        // Update stats
        stat.call_count.fetch_add(1, Ordering::Relaxed);
        stat.total_jitter_us.fetch_add(clamped as u64, Ordering::Relaxed);

        // Decide on decoy injection
        let mut decoys = Vec::new();

        if self.rng.gen::<f32>() < decoy_rate {
            let count = self.rng.gen_range(self.config.decoy_min_count..=self.config.decoy_max_count);
            for _ in 0..count {
                let all = DecoyType::all();
                decoys.push(all[self.rng.gen_range(0..all.len())]);
            }
            stat.decoy_injections.fetch_add(1, Ordering::Relaxed);
        }

        if self.rng.gen::<f32>() < self.config.global_decoy_rate {
            let all = DecoyType::all();
            decoys.push(all[self.rng.gen_range(0..all.len())]);
            self.total_decoys.fetch_add(1, Ordering::Relaxed);
        }

        SyscallPrep {
            jitter,
            decoys,
            category,
        }
    }

    /// Execute decoy syscalls. Call between the jitter sleep and the real syscall.
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
                    winapi::um::profileapi::QueryPerformanceCounter(
                        &mut counter as *mut i64 as *mut winapi::shared::ntdef::LARGE_INTEGER,
                    );
                }
                #[cfg(not(windows))]
                {
                    let _ = std::time::Instant::now();
                }
            }
            DecoyType::GlobalMemoryStatus => {
                #[cfg(windows)]
                unsafe {
                    let mut mem: winapi::um::sysinfoapi::MEMORYSTATUSEX = std::mem::zeroed();
                    mem.dwLength = std::mem::size_of::<winapi::um::sysinfoapi::MEMORYSTATUSEX>() as u32;
                    winapi::um::sysinfoapi::GlobalMemoryStatusEx(&mut mem);
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
                    let path = wide_str(
                        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
                    );
                    let _ = winapi::um::fileapi::GetFileAttributesW(path.as_ptr());
                }
                #[cfg(not(windows))]
                {
                    let _ = std::fs::metadata("/tmp/.chrome_lock");
                }
            }
            DecoyType::GetEnvironmentVariable => {
                let vars = ["TEMP", "PATH", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "CHROME_PATH"];
                let var = vars[self.rng.gen_range(0..vars.len())];
                let _ = std::env::var(var);
            }
            DecoyType::QueryThreadTimes => {
                #[cfg(windows)]
                unsafe {
                    let thread = winapi::um::processthreadsapi::GetCurrentThread();
                    let mut ct: winapi::shared::minwindef::FILETIME = std::mem::zeroed();
                    let mut et: winapi::shared::minwindef::FILETIME = std::mem::zeroed();
                    let mut kt: winapi::shared::minwindef::FILETIME = std::mem::zeroed();
                    let mut ut: winapi::shared::minwindef::FILETIME = std::mem::zeroed();
                    winapi::um::processthreadsapi::GetThreadTimes(
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
                    use winapi::um::winreg::*;
                    use winapi::um::winnt::KEY_READ;
                    let subkey = wide_str(r"SOFTWARE\Google\Chrome");
                    let mut hkey: winapi::shared::minwindef::HKEY = std::ptr::null_mut();
                    let result = RegOpenKeyExW(
                        HKEY_LOCAL_MACHINE,
                        subkey.as_ptr(),
                        0,
                        KEY_READ,
                        &mut hkey,
                    );
                    if result == 0 && !hkey.is_null() {
                        RegCloseKey(hkey);
                    }
                }
                #[cfg(not(windows))]
                {
                    let _ = std::fs::metadata("/etc/hostname");
                }
            }
        }
        self.total_decoys.fetch_add(1, Ordering::Relaxed);
    }

    /// Get stats for reporting
    pub fn get_stats(&self) -> CadenceReport {
        let cat_report = |s: &CategoryStats| -> CadenceCategoryReport {
            let count = s.call_count.load(Ordering::Relaxed);
            let total_jitter = s.total_jitter_us.load(Ordering::Relaxed);
            CadenceCategoryReport {
                call_count: count,
                avg_jitter_us: if count > 0 { total_jitter / count } else { 0 },
                decoy_injections: s.decoy_injections.load(Ordering::Relaxed),
            }
        };

        CadenceReport {
            memory: cat_report(&self.memory_stats),
            file_io: cat_report(&self.file_io_stats),
            total_decoys_executed: self.total_decoys.load(Ordering::Relaxed),
        }
    }
}

/// Preparation result — tells the caller what to do before the real syscall
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
    pub memory: CadenceCategoryReport,
    pub file_io: CadenceCategoryReport,
    pub total_decoys_executed: u64,
}

#[derive(Debug, Clone)]
pub struct CadenceCategoryReport {
    pub call_count: u64,
    pub avg_jitter_us: u64,
    pub decoy_injections: u64,
}

/// Convert a &str to a null-terminated wide string
#[cfg(windows)]
fn wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CadenceConfig::default();
        assert!(config.memory.jitter_mean_us > 0.0);
        assert!(config.memory.decoy_injection_rate > 0.0);
        assert!(config.global_decoy_rate > 0.0);
    }

    #[test]
    fn test_jitter_in_range() {
        let mut cadence = SyscallCadence::new(CadenceConfig::default());

        for _ in 0..1000 {
            let prep = cadence.pre_syscall(SyscallCategory::Memory);
            let us = prep.jitter.as_micros() as u64;
            assert!(
                (10..=600).contains(&us),
                "jitter {}µs outside memory range [10, 600]",
                us
            );
        }
    }

    #[test]
    fn test_jitter_distribution() {
        let mut cadence = SyscallCadence::new(CadenceConfig::default());
        let mut jitters = Vec::with_capacity(10000);

        for _ in 0..10000 {
            let prep = cadence.pre_syscall(SyscallCategory::Memory);
            jitters.push(prep.jitter.as_micros() as f64);
        }

        let mean = jitters.iter().sum::<f64>() / jitters.len() as f64;
        let variance = jitters.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / jitters.len() as f64;
        let stddev = variance.sqrt();

        // Mean should be near config (60µs)
        assert!(mean > 30.0 && mean < 90.0, "mean {:.1}µs outside expected range", mean);
        // Should have variance
        assert!(stddev > 5.0, "stddev too low: {:.1}µs", stddev);
    }

    #[test]
    fn test_decoy_injection_occurs() {
        let config = CadenceConfig {
            memory: CategoryConfig {
                decoy_injection_rate: 0.5, // High rate for testing
                ..Default::default()
            },
            global_decoy_rate: 0.0,
            ..Default::default()
        };
        let mut cadence = SyscallCadence::new(config);

        let mut decoy_count = 0;
        for _ in 0..100 {
            let prep = cadence.pre_syscall(SyscallCategory::Memory);
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
            let prep = cadence.pre_syscall(SyscallCategory::Memory);
            cadence.execute_decoys(&prep);
        }
        for _ in 0..30 {
            let prep = cadence.pre_syscall(SyscallCategory::FileIO);
            cadence.execute_decoys(&prep);
        }

        let report = cadence.get_stats();
        assert_eq!(report.memory.call_count, 50);
        assert_eq!(report.file_io.call_count, 30);
        assert!(report.memory.avg_jitter_us > 0);
    }

    #[test]
    fn test_no_decoys_when_rate_zero() {
        let config = CadenceConfig {
            memory: CategoryConfig {
                decoy_injection_rate: 0.0,
                ..Default::default()
            },
            global_decoy_rate: 0.0,
            ..Default::default()
        };
        let mut cadence = SyscallCadence::new(config);

        for _ in 0..1000 {
            let prep = cadence.pre_syscall(SyscallCategory::Memory);
            assert!(prep.decoys.is_empty(), "no decoys at 0% rate");
        }
    }
}
