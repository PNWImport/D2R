//! Handle Table Defense
//!
//! Prevents handle-table fingerprinting by managing Desktop Duplication
//! and other sensitive handles with lifecycle controls.
//!
//! Problem: A process with a persistent Desktop Duplication handle +
//! high-frequency SendInput to the D2 window = suspicious correlation
//! visible via NtQuerySystemInformation handle enumeration.
//!
//! Solution: Acquire handles in bursts, release between bursts,
//! and track handle lifetimes to ensure they don't persist
//! suspiciously long.

use std::time::{Duration, Instant};

/// Handle lifecycle state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HandleState {
    /// Handle not currently held
    Released,
    /// Handle acquired, actively being used
    Active,
    /// Handle marked for release after current operation
    PendingRelease,
}

/// Tracks a single managed handle
#[derive(Debug)]
pub struct ManagedHandle {
    pub name: String,
    pub state: HandleState,
    pub acquired_at: Option<Instant>,
    pub released_at: Option<Instant>,
    pub total_acquisitions: u64,
    pub total_active_time: Duration,
    pub max_hold_duration: Duration,
    pub current_hold_start: Option<Instant>,
}

impl ManagedHandle {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: HandleState::Released,
            acquired_at: None,
            released_at: None,
            total_acquisitions: 0,
            total_active_time: Duration::ZERO,
            max_hold_duration: Duration::ZERO,
            current_hold_start: None,
        }
    }

    pub fn acquire(&mut self) -> bool {
        if self.state != HandleState::Released {
            return false;
        }
        let now = Instant::now();
        self.state = HandleState::Active;
        self.acquired_at = Some(now);
        self.current_hold_start = Some(now);
        self.total_acquisitions += 1;
        true
    }

    pub fn release(&mut self) -> Duration {
        let hold_time = self
            .current_hold_start
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO);

        self.state = HandleState::Released;
        self.released_at = Some(Instant::now());
        self.total_active_time += hold_time;
        if hold_time > self.max_hold_duration {
            self.max_hold_duration = hold_time;
        }
        self.current_hold_start = None;
        hold_time
    }

    pub fn current_hold_time(&self) -> Duration {
        self.current_hold_start
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    pub fn is_active(&self) -> bool {
        self.state == HandleState::Active
    }

    pub fn avg_hold_time(&self) -> Duration {
        if self.total_acquisitions == 0 {
            Duration::ZERO
        } else {
            self.total_active_time / self.total_acquisitions as u32
        }
    }
}

/// Manages all sensitive handles with burst lifecycle
pub struct HandleManager {
    handles: Vec<ManagedHandle>,
    /// Max time any single handle should be held
    max_hold_ms: u64,
    /// Check interval for forced release
    pub check_interval: Duration,
    last_check: Instant,
}

impl HandleManager {
    pub fn new(max_hold_ms: u64) -> Self {
        Self {
            handles: Vec::new(),
            max_hold_ms,
            check_interval: Duration::from_secs(1),
            last_check: Instant::now(),
        }
    }

    pub fn register(&mut self, name: impl Into<String>) -> usize {
        let idx = self.handles.len();
        self.handles.push(ManagedHandle::new(name));
        idx
    }

    pub fn acquire(&mut self, idx: usize) -> bool {
        if idx < self.handles.len() {
            self.handles[idx].acquire()
        } else {
            false
        }
    }

    pub fn release(&mut self, idx: usize) -> Duration {
        if idx < self.handles.len() {
            self.handles[idx].release()
        } else {
            Duration::ZERO
        }
    }

    pub fn is_active(&self, idx: usize) -> bool {
        self.handles.get(idx).is_some_and(|h| h.is_active())
    }

    /// Check all handles and force-release any held too long
    pub fn enforce_limits(&mut self) -> Vec<(usize, Duration)> {
        let now = Instant::now();
        if now.duration_since(self.last_check) < self.check_interval {
            return Vec::new();
        }
        self.last_check = now;

        let max_hold = Duration::from_millis(self.max_hold_ms);
        let mut forced = Vec::new();

        for (idx, handle) in self.handles.iter_mut().enumerate() {
            if handle.is_active() && handle.current_hold_time() > max_hold {
                let held = handle.release();
                forced.push((idx, held));
            }
        }

        forced
    }

    pub fn report(&self) -> Vec<HandleReport> {
        self.handles
            .iter()
            .map(|h| HandleReport {
                name: h.name.clone(),
                state: h.state,
                total_acquisitions: h.total_acquisitions,
                avg_hold_time: h.avg_hold_time(),
                max_hold_time: h.max_hold_duration,
                current_hold_time: h.current_hold_time(),
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct HandleReport {
    pub name: String,
    pub state: HandleState,
    pub total_acquisitions: u64,
    pub avg_hold_time: Duration,
    pub max_hold_time: Duration,
    pub current_hold_time: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_lifecycle() {
        let mut h = ManagedHandle::new("desktop_duplication");
        assert_eq!(h.state, HandleState::Released);

        assert!(h.acquire());
        assert_eq!(h.state, HandleState::Active);
        assert_eq!(h.total_acquisitions, 1);

        // Can't double-acquire
        assert!(!h.acquire());

        std::thread::sleep(Duration::from_millis(10));
        let held = h.release();
        assert!(held >= Duration::from_millis(10));
        assert_eq!(h.state, HandleState::Released);
        assert_eq!(h.total_acquisitions, 1);
        assert!(h.total_active_time >= Duration::from_millis(10));
    }

    #[test]
    fn test_manager_registration() {
        let mut mgr = HandleManager::new(5000);
        let idx0 = mgr.register("desktop_dup");
        let idx1 = mgr.register("send_input_handle");
        assert_eq!(idx0, 0);
        assert_eq!(idx1, 1);
    }

    #[test]
    fn test_manager_acquire_release() {
        let mut mgr = HandleManager::new(5000);
        let idx = mgr.register("test_handle");

        assert!(mgr.acquire(idx));
        assert!(mgr.is_active(idx));

        let held = mgr.release(idx);
        assert!(!mgr.is_active(idx));
        assert!(held < Duration::from_secs(1));
    }

    #[test]
    fn test_enforce_limits() {
        let mut mgr = HandleManager::new(50); // 50ms max hold
        mgr.check_interval = Duration::ZERO; // Check immediately
        let idx = mgr.register("short_lived");

        mgr.acquire(idx);
        std::thread::sleep(Duration::from_millis(60));

        let forced = mgr.enforce_limits();
        assert_eq!(forced.len(), 1);
        assert_eq!(forced[0].0, idx);
        assert!(forced[0].1 >= Duration::from_millis(50));
        assert!(!mgr.is_active(idx));
    }

    #[test]
    fn test_multiple_acquire_release_cycles() {
        let mut h = ManagedHandle::new("cycled_handle");

        for i in 0..10 {
            assert!(h.acquire(), "acquire failed on cycle {}", i);
            std::thread::sleep(Duration::from_millis(5));
            h.release();
        }

        assert_eq!(h.total_acquisitions, 10);
        assert!(h.total_active_time >= Duration::from_millis(50));
        assert!(h.avg_hold_time() >= Duration::from_millis(5));
    }

    #[test]
    fn test_report() {
        let mut mgr = HandleManager::new(5000);
        let idx = mgr.register("test");
        mgr.acquire(idx);
        std::thread::sleep(Duration::from_millis(10));
        mgr.release(idx);

        let reports = mgr.report();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].name, "test");
        assert_eq!(reports[0].total_acquisitions, 1);
        assert_eq!(reports[0].state, HandleState::Released);
    }
}
