//! Thread-Rotated Input Dispatch
//!
//! Rotates SendInput calls across N worker threads to prevent call-site
//! fingerprinting via ETW. Each thread has unique stack layout (randomized
//! padding at spawn), giving different return addresses in kernel traces.
//!
//! On Windows: Real SendInput API calls with per-thread jitter.
//! On Linux: Timing stubs for test validation of dispatch logic.

use rand::prelude::*;
use rand_distr::{Distribution, Normal};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum InputCommand {
    KeyPress {
        key: char,
        hold_ms: u64,
    },
    MouseMove {
        x: i32,
        y: i32,
    },
    LeftClick {
        hold_ms: u64,
    },
    RightClick {
        hold_ms: u64,
    },
    ClickAt {
        x: i32,
        y: i32,
        button: MouseButton,
        hold_ms: u64,
    },
    Shutdown,
}

#[derive(Debug, Clone, Copy)]
pub enum MouseButton {
    Left,
    Right,
}

#[derive(Debug, Default)]
struct ThreadStats {
    commands_processed: AtomicU64,
    total_latency_us: AtomicU64,
}

struct InputWorker {
    _id: usize,
    rx: mpsc::Receiver<InputCommand>,
    stats: std::sync::Arc<ThreadStats>,
    timing_dist: Normal<f64>,
    _stack_padding: Vec<u8>,
}

impl InputWorker {
    fn run(self) {
        let mut rng = thread_rng();
        while let Ok(cmd) = self.rx.recv() {
            match cmd {
                InputCommand::Shutdown => break,
                cmd => {
                    let start = Instant::now();

                    // Per-thread jitter before dispatch
                    let extra_us = self.timing_dist.sample(&mut rng).max(0.0) as u64;
                    if extra_us > 0 {
                        thread::sleep(Duration::from_micros(extra_us));
                    }

                    Self::execute(&cmd, &mut rng);

                    self.stats
                        .commands_processed
                        .fetch_add(1, Ordering::Relaxed);
                    self.stats
                        .total_latency_us
                        .fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);
                }
            }
        }
    }

    #[cfg(windows)]
    fn execute(cmd: &InputCommand, rng: &mut ThreadRng) {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        use windows::Win32::UI::WindowsAndMessaging::*;

        match cmd {
            InputCommand::KeyPress { key, hold_ms } => {
                let vk = char_to_vk(*key);
                let hold = jitter_ms(*hold_ms, rng);
                unsafe {
                    let mut down = INPUT::default();
                    down.r#type = INPUT_KEYBOARD;
                    down.Anonymous.ki.wVk = VIRTUAL_KEY(vk);
                    SendInput(&[down], std::mem::size_of::<INPUT>() as i32);

                    thread::sleep(Duration::from_millis(hold));

                    let mut up = INPUT::default();
                    up.r#type = INPUT_KEYBOARD;
                    up.Anonymous.ki.wVk = VIRTUAL_KEY(vk);
                    up.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
                    SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
                }
            }
            InputCommand::MouseMove { x, y } => {
                unsafe {
                    let _ = SetCursorPos(*x, *y);
                }
                thread::sleep(Duration::from_micros(rng.gen_range(50..300)));
            }
            InputCommand::LeftClick { hold_ms } => {
                let hold = jitter_ms(*hold_ms, rng);
                unsafe {
                    let mut down = INPUT::default();
                    down.r#type = INPUT_MOUSE;
                    down.Anonymous.mi.dwFlags = MOUSEEVENTF_LEFTDOWN;
                    SendInput(&[down], std::mem::size_of::<INPUT>() as i32);

                    thread::sleep(Duration::from_millis(hold));

                    let mut up = INPUT::default();
                    up.r#type = INPUT_MOUSE;
                    up.Anonymous.mi.dwFlags = MOUSEEVENTF_LEFTUP;
                    SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
                }
            }
            InputCommand::RightClick { hold_ms } => {
                let hold = jitter_ms(*hold_ms, rng);
                unsafe {
                    let mut down = INPUT::default();
                    down.r#type = INPUT_MOUSE;
                    down.Anonymous.mi.dwFlags = MOUSEEVENTF_RIGHTDOWN;
                    SendInput(&[down], std::mem::size_of::<INPUT>() as i32);

                    thread::sleep(Duration::from_millis(hold));

                    let mut up = INPUT::default();
                    up.r#type = INPUT_MOUSE;
                    up.Anonymous.mi.dwFlags = MOUSEEVENTF_RIGHTUP;
                    SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
                }
            }
            InputCommand::ClickAt {
                x,
                y,
                button,
                hold_ms,
            } => {
                unsafe {
                    let _ = SetCursorPos(*x, *y);
                }
                thread::sleep(Duration::from_millis(rng.gen_range(5..25)));
                match button {
                    MouseButton::Left => {
                        Self::execute(&InputCommand::LeftClick { hold_ms: *hold_ms }, rng)
                    }
                    MouseButton::Right => {
                        Self::execute(&InputCommand::RightClick { hold_ms: *hold_ms }, rng)
                    }
                }
            }
            InputCommand::Shutdown => unreachable!(),
        }
    }

    #[cfg(not(windows))]
    fn execute(cmd: &InputCommand, rng: &mut ThreadRng) {
        // Timing-accurate stubs — same delays, no hardware interaction
        match cmd {
            InputCommand::KeyPress { hold_ms, .. } => {
                thread::sleep(Duration::from_millis(jitter_ms(*hold_ms, rng)));
            }
            InputCommand::MouseMove { .. } => {
                thread::sleep(Duration::from_micros(rng.gen_range(50..300)));
            }
            InputCommand::LeftClick { hold_ms } | InputCommand::RightClick { hold_ms } => {
                thread::sleep(Duration::from_millis(jitter_ms(*hold_ms, rng)));
            }
            InputCommand::ClickAt { hold_ms, .. } => {
                thread::sleep(Duration::from_millis(rng.gen_range(5..25)));
                thread::sleep(Duration::from_millis(jitter_ms(*hold_ms, rng)));
            }
            InputCommand::Shutdown => unreachable!(),
        }
    }
}

fn jitter_ms(base: u64, rng: &mut ThreadRng) -> u64 {
    (base as f64 * rng.gen_range(0.8..1.2)).max(15.0) as u64
}

#[cfg(windows)]
fn char_to_vk(c: char) -> u16 {
    match c {
        '0'..='9' => 0x30 + (c as u16 - '0' as u16),
        'a'..='z' => 0x41 + (c as u16 - 'a' as u16),
        'A'..='Z' => 0x41 + (c as u16 - 'A' as u16),
        '\x1b' => 0x1B, // Escape
        '\r' => 0x0D,   // Enter
        '\t' => 0x09,   // Tab
        ' ' => 0x20,    // Space
        // Function keys: mapped to chars \x80-\x8B (F1-F12)
        '\u{80}' => 0x70, // VK_F1
        '\u{81}' => 0x71, // VK_F2
        '\u{82}' => 0x72, // VK_F3
        '\u{83}' => 0x73, // VK_F4
        '\u{84}' => 0x74, // VK_F5
        '\u{85}' => 0x75, // VK_F6
        '\u{86}' => 0x76, // VK_F7
        '\u{87}' => 0x77, // VK_F8
        '\u{88}' => 0x78, // VK_F9
        '\u{89}' => 0x79, // VK_F10
        '\u{8A}' => 0x7A, // VK_F11
        '\u{8B}' => 0x7B, // VK_F12
        '-' => 0xBD,      // VK_OEM_MINUS
        '=' => 0xBB,      // VK_OEM_PLUS
        '[' => 0xDB,      // VK_OEM_4
        ']' => 0xDD,      // VK_OEM_6
        ',' => 0xBC,      // VK_OEM_COMMA
        '.' => 0xBE,      // VK_OEM_PERIOD
        '/' => 0xBF,      // VK_OEM_2
        ';' => 0xBA,      // VK_OEM_1
        '\'' => 0xDE,     // VK_OEM_7
        '`' => 0xC0,      // VK_OEM_3
        _ => c as u16,
    }
}

// ═══════════════════════════════════════════════════════════════
// ThreadRotatedInput — dispatch pool
// ═══════════════════════════════════════════════════════════════

pub struct ThreadRotatedInput {
    workers: Vec<mpsc::Sender<InputCommand>>,
    worker_stats: Vec<std::sync::Arc<ThreadStats>>,
    handles: Vec<Option<thread::JoinHandle<()>>>,
    current_worker: AtomicUsize,
    total_dispatched: AtomicU64,
    strategy: RotationStrategy,
}

#[derive(Debug, Clone, Copy)]
pub enum RotationStrategy {
    RoundRobin,
    Random,
    LeastRecentlyUsed,
}

#[derive(Debug, Clone)]
pub struct ThreadPoolConfig {
    pub num_workers: usize,
    pub timing_jitter_mean_us: f64,
    pub timing_jitter_stddev_us: f64,
    pub stack_padding_min: usize,
    pub stack_padding_max: usize,
    pub strategy: RotationStrategy,
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        Self {
            num_workers: 4,
            timing_jitter_mean_us: 200.0,
            timing_jitter_stddev_us: 80.0,
            stack_padding_min: 64,
            stack_padding_max: 4096,
            strategy: RotationStrategy::RoundRobin,
        }
    }
}

impl ThreadRotatedInput {
    pub fn new(config: ThreadPoolConfig) -> Self {
        let mut workers = Vec::with_capacity(config.num_workers);
        let mut handles = Vec::with_capacity(config.num_workers);
        let mut stats = Vec::with_capacity(config.num_workers);
        let mut rng = thread_rng();

        for id in 0..config.num_workers {
            let (tx, rx) = mpsc::channel();
            let thread_stats = std::sync::Arc::new(ThreadStats::default());

            let timing_dist =
                Normal::new(config.timing_jitter_mean_us, config.timing_jitter_stddev_us)
                    .unwrap_or_else(|_| Normal::new(200.0, 80.0).unwrap());

            let padding_size = rng.gen_range(config.stack_padding_min..=config.stack_padding_max);

            let worker = InputWorker {
                _id: id,
                rx,
                stats: thread_stats.clone(),
                timing_dist,
                _stack_padding: vec![0u8; padding_size],
            };

            let handle = thread::Builder::new()
                .name(format!("input-worker-{}", id))
                .stack_size(1024 * 1024 + rng.gen_range(0..64 * 1024))
                .spawn(move || worker.run())
                .expect("failed to spawn input worker");

            workers.push(tx);
            handles.push(Some(handle));
            stats.push(thread_stats);
        }

        Self {
            workers,
            worker_stats: stats,
            handles,
            current_worker: AtomicUsize::new(0),
            total_dispatched: AtomicU64::new(0),
            strategy: config.strategy,
        }
    }

    pub fn dispatch(&self, cmd: InputCommand) -> bool {
        let idx = self.select_worker();
        self.total_dispatched.fetch_add(1, Ordering::Relaxed);
        self.workers[idx].send(cmd).is_ok()
    }

    fn select_worker(&self) -> usize {
        let n = self.workers.len();
        match self.strategy {
            RotationStrategy::RoundRobin => self.current_worker.fetch_add(1, Ordering::Relaxed) % n,
            RotationStrategy::Random => thread_rng().gen_range(0..n),
            RotationStrategy::LeastRecentlyUsed => self
                .worker_stats
                .iter()
                .enumerate()
                .min_by_key(|(_, s)| s.commands_processed.load(Ordering::Relaxed))
                .map(|(i, _)| i)
                .unwrap_or(0),
        }
    }

    pub fn stats(&self) -> InputDispatchStats {
        InputDispatchStats {
            total_dispatched: self.total_dispatched.load(Ordering::Relaxed),
            per_thread: self
                .worker_stats
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let cmds = s.commands_processed.load(Ordering::Relaxed);
                    let lat = s.total_latency_us.load(Ordering::Relaxed);
                    ThreadDispatchStats {
                        thread_id: i,
                        commands_processed: cmds,
                        avg_latency_us: if cmds > 0 { lat / cmds } else { 0 },
                    }
                })
                .collect(),
        }
    }

    pub fn num_workers(&self) -> usize {
        self.workers.len()
    }
}

impl Drop for ThreadRotatedInput {
    fn drop(&mut self) {
        for tx in &self.workers {
            let _ = tx.send(InputCommand::Shutdown);
        }
        for h in &mut self.handles {
            if let Some(h) = h.take() {
                let _ = h.join();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct InputDispatchStats {
    pub total_dispatched: u64,
    pub per_thread: Vec<ThreadDispatchStats>,
}

#[derive(Debug, Clone)]
pub struct ThreadDispatchStats {
    pub thread_id: usize,
    pub commands_processed: u64,
    pub avg_latency_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_dispatch() {
        let pool = ThreadRotatedInput::new(ThreadPoolConfig::default());
        assert!(pool.dispatch(InputCommand::KeyPress {
            key: 'f',
            hold_ms: 50
        }));
        thread::sleep(Duration::from_millis(200));
        assert_eq!(pool.stats().total_dispatched, 1);
    }

    #[test]
    fn test_round_robin_distributes() {
        let pool = ThreadRotatedInput::new(ThreadPoolConfig {
            num_workers: 4,
            strategy: RotationStrategy::RoundRobin,
            ..Default::default()
        });
        for i in 0..100 {
            pool.dispatch(InputCommand::KeyPress {
                key: (b'a' + (i % 26) as u8) as char,
                hold_ms: 20,
            });
        }
        thread::sleep(Duration::from_millis(500));
        let stats = pool.stats();
        assert_eq!(stats.total_dispatched, 100);
        for ts in &stats.per_thread {
            assert!(ts.commands_processed >= 15 && ts.commands_processed <= 35);
        }
    }

    #[test]
    fn test_multiple_command_types() {
        let pool = ThreadRotatedInput::new(ThreadPoolConfig::default());
        pool.dispatch(InputCommand::KeyPress {
            key: 'f',
            hold_ms: 50,
        });
        pool.dispatch(InputCommand::MouseMove { x: 400, y: 300 });
        pool.dispatch(InputCommand::LeftClick { hold_ms: 40 });
        pool.dispatch(InputCommand::RightClick { hold_ms: 35 });
        pool.dispatch(InputCommand::ClickAt {
            x: 500,
            y: 200,
            button: MouseButton::Right,
            hold_ms: 45,
        });
        thread::sleep(Duration::from_millis(500));
        let total: u64 = pool
            .stats()
            .per_thread
            .iter()
            .map(|t| t.commands_processed)
            .sum();
        assert_eq!(total, 5);
    }

    #[test]
    fn test_thread_entropy() {
        let pool = ThreadRotatedInput::new(ThreadPoolConfig {
            num_workers: 4,
            strategy: RotationStrategy::RoundRobin,
            ..Default::default()
        });
        for _ in 0..8 {
            pool.dispatch(InputCommand::KeyPress {
                key: 'a',
                hold_ms: 15,
            });
        }
        thread::sleep(Duration::from_millis(300));
        let active = pool
            .stats()
            .per_thread
            .iter()
            .filter(|t| t.commands_processed > 0)
            .count();
        assert_eq!(active, 4);
    }

    #[test]
    fn test_clean_shutdown() {
        let pool = ThreadRotatedInput::new(ThreadPoolConfig::default());
        for _ in 0..20 {
            pool.dispatch(InputCommand::KeyPress {
                key: 'x',
                hold_ms: 10,
            });
        }
        thread::sleep(Duration::from_millis(100));
        drop(pool);
    }

    #[test]
    fn test_high_throughput() {
        let pool = ThreadRotatedInput::new(ThreadPoolConfig {
            num_workers: 4,
            timing_jitter_mean_us: 0.0,
            timing_jitter_stddev_us: 0.1,
            ..Default::default()
        });
        for i in 0..1000u32 {
            pool.dispatch(InputCommand::KeyPress {
                key: (b'a' + (i % 26) as u8) as char,
                hold_ms: 5,
            });
        }
        thread::sleep(Duration::from_millis(2000));
        let total: u64 = pool
            .stats()
            .per_thread
            .iter()
            .map(|t| t.commands_processed)
            .sum();
        assert!(total >= 200);
    }
}
