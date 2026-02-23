//! Human-like input simulation.
//!
//! On Windows: Real SendInput API calls with bezier mouse paths,
//! gaussian key hold durations, and overshoot corrections.
//!
//! On non-Windows: Simulation stubs that log actions and respect timing.

use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use std::time::Duration;

pub struct InputSimulator {
    rng: StdRng,
    key_hold_dist: Normal<f64>, // Key hold duration ms
    cursor_x: i32,
    cursor_y: i32,
}

impl Default for InputSimulator {
    fn default() -> Self {
        Self::new()
    }
}

impl InputSimulator {
    pub fn new() -> Self {
        Self {
            rng: StdRng::from_entropy(),
            key_hold_dist: Normal::new(55.0, 18.0).unwrap(),
            cursor_x: 400,
            cursor_y: 300,
        }
    }

    /// Move mouse along a cubic bezier curve to target.
    pub async fn move_mouse_to(&mut self, target_x: i32, target_y: i32) {
        let (sx, sy) = self.get_cursor_pos();
        let dist = (((target_x - sx).pow(2) + (target_y - sy).pow(2)) as f64).sqrt();

        if dist < 3.0 {
            return; // Already there
        }

        // Cubic bezier control points with randomness
        let cp1 = (
            sx + (target_x - sx) / 3 + self.rng.gen_range(-40..40),
            sy + (target_y - sy) / 3 + self.rng.gen_range(-25..25),
        );
        let cp2 = (
            sx + 2 * (target_x - sx) / 3 + self.rng.gen_range(-40..40),
            sy + 2 * (target_y - sy) / 3 + self.rng.gen_range(-25..25),
        );

        let steps = (dist / 15.0).clamp(6.0, 35.0) as u32;

        for i in 0..=steps {
            let t = i as f64 / steps as f64;

            // Cubic bezier
            let bx = (1.0 - t).powi(3) * sx as f64
                + 3.0 * (1.0 - t).powi(2) * t * cp1.0 as f64
                + 3.0 * (1.0 - t) * t.powi(2) * cp2.0 as f64
                + t.powi(3) * target_x as f64;
            let by = (1.0 - t).powi(3) * sy as f64
                + 3.0 * (1.0 - t).powi(2) * t * cp1.1 as f64
                + 3.0 * (1.0 - t) * t.powi(2) * cp2.1 as f64
                + t.powi(3) * target_y as f64;

            self.set_cursor_pos(bx as i32, by as i32);

            // Variable speed: sinusoidal (slow-fast-slow)
            let speed = (t * std::f64::consts::PI).sin();
            let delay_ms = (1.5 + 6.0 * (1.0 - speed)) as u64;
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        // 15% chance of overshoot + correction
        if self.rng.gen::<f32>() < 0.15 {
            let ox = target_x + self.rng.gen_range(-8..8);
            let oy = target_y + self.rng.gen_range(-6..6);
            self.set_cursor_pos(ox, oy);
            tokio::time::sleep(Duration::from_millis(self.rng.gen_range(30..80))).await;
            self.set_cursor_pos(target_x, target_y);
        }
    }

    /// Left click with human-like hold duration
    pub async fn left_click(&mut self, x: i32, y: i32) {
        self.move_mouse_to(x, y).await;
        let hold = self.key_hold_dist.sample(&mut self.rng).max(25.0) as u64;
        self.mouse_down(true);
        tokio::time::sleep(Duration::from_millis(hold)).await;
        self.mouse_up(true);
    }

    /// Right click (movement, skills)
    pub async fn right_click(&mut self, x: i32, y: i32) {
        self.move_mouse_to(x, y).await;
        let hold = self.key_hold_dist.sample(&mut self.rng).max(25.0) as u64;
        self.mouse_down(false);
        tokio::time::sleep(Duration::from_millis(hold)).await;
        self.mouse_up(false);
    }

    /// Press a key with human-like timing
    pub async fn press_key(&mut self, key: char) {
        let hold = self.key_hold_dist.sample(&mut self.rng).max(25.0) as u64;
        self.key_down(key);
        tokio::time::sleep(Duration::from_millis(hold)).await;
        self.key_up(key);
    }

    /// Cast skill at position: hotkey then right-click
    pub async fn cast_at(&mut self, skill_key: char, x: i32, y: i32) {
        self.press_key(skill_key).await;
        let gap = self.rng.gen_range(30..90);
        tokio::time::sleep(Duration::from_millis(gap)).await;
        self.right_click(x, y).await;
    }

    /// Use belt potion (keys 1-4)
    pub async fn use_belt(&mut self, slot: u8) {
        let key = match slot {
            0 => '1',
            1 => '2',
            2 => '3',
            3 => '4',
            _ => return,
        };
        self.press_key(key).await;
    }

    // ─── Platform Internals ────────────────────────────────────

    fn get_cursor_pos(&self) -> (i32, i32) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::POINT;
            use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
            unsafe {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                (pt.x, pt.y)
            }
        }
        #[cfg(not(windows))]
        {
            (self.cursor_x, self.cursor_y)
        }
    }

    fn set_cursor_pos(&mut self, x: i32, y: i32) {
        self.cursor_x = x;
        self.cursor_y = y;

        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
            unsafe {
                let _ = SetCursorPos(x, y);
            }
        }
    }

    fn mouse_down(&self, _left: bool) {
        #[cfg(windows)]
        {
            use windows::Win32::UI::Input::KeyboardAndMouse::*;
            let flags = if left {
                MOUSEEVENTF_LEFTDOWN
            } else {
                MOUSEEVENTF_RIGHTDOWN
            };
            unsafe {
                let mut input = INPUT::default();
                input.r#type = INPUT_MOUSE;
                input.Anonymous.mi.dwFlags = flags;
                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            }
        }
    }

    fn mouse_up(&self, _left: bool) {
        #[cfg(windows)]
        {
            use windows::Win32::UI::Input::KeyboardAndMouse::*;
            let flags = if left {
                MOUSEEVENTF_LEFTUP
            } else {
                MOUSEEVENTF_RIGHTUP
            };
            unsafe {
                let mut input = INPUT::default();
                input.r#type = INPUT_MOUSE;
                input.Anonymous.mi.dwFlags = flags;
                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            }
        }
    }

    fn key_down(&self, _key: char) {
        #[cfg(windows)]
        {
            use windows::Win32::UI::Input::KeyboardAndMouse::*;
            unsafe {
                let mut input = INPUT::default();
                input.r#type = INPUT_KEYBOARD;
                input.Anonymous.ki.wVk = VIRTUAL_KEY(char_to_vk(key));
                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            }
        }
    }

    fn key_up(&self, _key: char) {
        #[cfg(windows)]
        {
            use windows::Win32::UI::Input::KeyboardAndMouse::*;
            unsafe {
                let mut input = INPUT::default();
                input.r#type = INPUT_KEYBOARD;
                input.Anonymous.ki.wVk = VIRTUAL_KEY(char_to_vk(key));
                input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            }
        }
    }
}

#[allow(dead_code)]
fn char_to_vk(c: char) -> u16 {
    match c {
        '0'..='9' => 0x30 + (c as u16 - '0' as u16),
        'a'..='z' => 0x41 + (c as u16 - 'a' as u16),
        'A'..='Z' => 0x41 + (c as u16 - 'A' as u16),
        _ => c as u16,
    }
}
