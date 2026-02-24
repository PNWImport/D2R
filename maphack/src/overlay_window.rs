// =============================================================================
// Debug Overlay Window — Win32 Layered Topmost Window
// =============================================================================
// Creates a transparent, click-through, always-on-top window positioned over
// D2R's client area. Draws detection boxes (HP orb, MP orb, enemy positions)
// using GDI into a memory DC, then blits via UpdateLayeredWindow.
//
// The window lives on its own thread (Win32 message pump). The main thread
// calls update() to push new debug state; the window thread redraws on WM_TIMER.
//
// Non-Windows: all functions are stubs that do nothing.
// =============================================================================

/// Snapshot of vision-agent detection state passed to the overlay for drawing.
/// Fields are read by the Win32 overlay painter on Windows; on other platforms
/// this struct is only constructed/stored, so the fields appear unused.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct DebugState {
    pub hp_pct: u8,
    pub mp_pct: u8,
    pub merc_hp_pct: u8,
    pub enemy_count: u8,
    pub nearest_enemy_x: u16,
    pub nearest_enemy_y: u16,
    pub nearest_enemy_hp_pct: u8,
    pub chicken_hp_pct: u8,
    pub area_name: String,
    pub in_game: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Windows implementation
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(windows)]
mod win_impl {
    use super::DebugState;
    use std::sync::{Arc, Mutex};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use winapi::shared::windef::{HWND, RECT, POINT};
    use winapi::shared::minwindef::{BOOL, TRUE, FALSE, LPARAM, WPARAM, UINT, LRESULT, DWORD};
    use winapi::um::winuser::*;
    use winapi::um::wingdi::*;
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::shared::windef::HBRUSH;

    const CLASS_NAME: &str = "KzbDebugOverlay\0";
    const WINDOW_TITLE: &str = "KZB Debug\0";
    const REDRAW_TIMER_ID: usize = 1;
    const REDRAW_INTERVAL_MS: u32 = 100; // 10 fps — plenty for debug

    // Color key: this exact RGB is treated as transparent by the layered window.
    // Anything drawn in this color will be invisible; all other colors are opaque.
    const COLORKEY_R: u8 = 0;
    const COLORKEY_G: u8 = 0;
    const COLORKEY_B: u8 = 0;

    // Debug box colors (GDI COLORREF = 0x00BBGGRR)
    const COLOR_HP:     DWORD = 0x00_00_40_C0; // red-ish
    const COLOR_MP:     DWORD = 0x00_C0_40_00; // blue-ish
    const COLOR_ENEMY:  DWORD = 0x00_00_C0_C0; // yellow
    const COLOR_CHICK:  DWORD = 0x00_00_C0_FF; // orange
    const COLOR_TEXT:   DWORD = 0x00_FF_FF_FF; // white
    const COLOR_KEY:    DWORD = 0x00_00_00_00; // transparent (black colorkey)

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Handle to the overlay window thread.
    pub struct OverlayWindow {
        hwnd: HWND,
        state: Arc<Mutex<DebugState>>,
        _thread: std::thread::JoinHandle<()>,
    }

    // SAFETY: HWND is just a pointer integer; we only use it from the correct thread.
    unsafe impl Send for OverlayWindow {}
    unsafe impl Sync for OverlayWindow {}

    impl OverlayWindow {
        /// Spawn the overlay window. Blocks briefly until the window is created.
        pub fn create() -> Option<Self> {
            let state = Arc::new(Mutex::new(DebugState::default()));
            let state_clone = Arc::clone(&state);

            // One-shot channel so we can get the HWND back from the window thread.
            let (tx, rx) = std::sync::mpsc::channel::<HWND>();

            let thread = std::thread::Builder::new()
                .name("kzb_debug_overlay".into())
                .spawn(move || {
                    let hwnd = unsafe { create_overlay_window() };
                    let _ = tx.send(hwnd);
                    if hwnd.is_null() { return; }

                    // Store state pointer in window userdata for access in WndProc.
                    let state_ptr = Box::into_raw(Box::new(Arc::clone(&state_clone)));
                    unsafe {
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
                        // Start redraw timer
                        SetTimer(hwnd, REDRAW_TIMER_ID, REDRAW_INTERVAL_MS, None);
                    }

                    // Message pump
                    let mut msg: MSG = unsafe { std::mem::zeroed() };
                    loop {
                        let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
                        if ret <= 0 { break; }
                        unsafe {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }

                    // Cleanup: reclaim state Arc
                    unsafe {
                        let _ = Box::from_raw(state_ptr);
                    }
                })
                .ok()?;

            let hwnd = rx.recv_timeout(std::time::Duration::from_secs(3)).ok()?;
            if hwnd.is_null() { return None; }

            Some(OverlayWindow { hwnd, state, _thread: thread })
        }

        /// Push new detection state. The window thread redraws on its next timer tick.
        pub fn update(&self, new_state: DebugState) {
            if let Ok(mut s) = self.state.lock() {
                *s = new_state;
            }
        }

        /// Hide and destroy the overlay window.
        pub fn destroy(self) {
            unsafe { PostMessageW(self.hwnd, WM_CLOSE, 0, 0); }
            // Thread join is implicit via Drop of _thread (detached if not joined)
        }
    }

    /// Register class and create the layered topmost window.
    unsafe fn create_overlay_window() -> HWND {
        let hinstance = GetModuleHandleW(std::ptr::null());

        let class_name = wide(CLASS_NAME);

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as UINT,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);

        // Find D2R window to size/position overlay
        let d2r_name = wide("Diablo II: Resurrected");
        let d2r_hwnd = FindWindowW(std::ptr::null(), d2r_name.as_ptr());
        let (x, y, w, h) = if !d2r_hwnd.is_null() {
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(d2r_hwnd, &mut rect);
            let mut pt = POINT { x: rect.left, y: rect.top };
            ClientToScreen(d2r_hwnd, &mut pt);
            (pt.x, pt.y, rect.right - rect.left, rect.bottom - rect.top)
        } else {
            // D2R not found — create a small diagnostic window
            (50, 50, 400, 300)
        };

        let title = wide(WINDOW_TITLE);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            x, y, w, h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null_mut(),
        );

        if !hwnd.is_null() {
            // Black (0,0,0) is the color key → transparent; alpha doesn't matter for colorkey mode
            SetLayeredWindowAttributes(hwnd, RGB(COLORKEY_R, COLORKEY_G, COLORKEY_B), 0, LWA_COLORKEY);
        }
        hwnd
    }

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_TIMER => {
                if wparam == REDRAW_TIMER_ID {
                    redraw(hwnd);
                }
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, REDRAW_TIMER_ID);
                PostQuitMessage(0);
                0
            }
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(hwnd, &mut ps);
                redraw_hdc(hwnd, hdc);
                EndPaint(hwnd, &ps);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe fn redraw(hwnd: HWND) {
        let hdc = GetDC(hwnd);
        if hdc.is_null() { return; }
        redraw_hdc(hwnd, hdc);
        ReleaseDC(hwnd, hdc);
    }

    unsafe fn redraw_hdc(hwnd: HWND, hdc: winapi::shared::windef::HDC) {
        // Get current debug state
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Arc<Mutex<DebugState>>;
        let state = if ptr.is_null() {
            DebugState::default()
        } else {
            match (*ptr).lock() {
                Ok(s) => s.clone(),
                Err(_) => DebugState::default(),
            }
        };

        // Window dimensions
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let w = rect.right;
        let h = rect.bottom;

        // Create memory DC + bitmap for flicker-free drawing
        let mem_dc = CreateCompatibleDC(hdc);
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old_bmp = SelectObject(mem_dc, bmp as _);

        // Fill entire window with color key (transparent)
        let bg_brush = CreateSolidBrush(COLOR_KEY) as HBRUSH;
        FillRect(mem_dc, &rect, bg_brush);
        DeleteObject(bg_brush as _);

        if state.in_game {
            draw_debug_state(mem_dc, w, h, &state);
        } else {
            draw_standby_text(mem_dc, w, h);
        }

        // Blit to screen
        BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);

        SelectObject(mem_dc, old_bmp);
        DeleteObject(bmp as _);
        DeleteDC(mem_dc);
    }

    unsafe fn draw_debug_state(hdc: winapi::shared::windef::HDC, w: i32, h: i32, s: &DebugState) {
        // ── HP bar indicator (left side, top) ──────────────────────────────────
        let hp_bar_w = ((s.hp_pct as i32) * 120 / 100).max(2);
        let hp_rect = RECT { left: 10, top: 10, right: 10 + hp_bar_w, bottom: 26 };
        let hp_brush = CreateSolidBrush(COLOR_HP) as HBRUSH;
        FillRect(hdc, &hp_rect, hp_brush);
        DeleteObject(hp_brush as _);
        draw_text_at(hdc, 135, 10, &format!("HP {}%", s.hp_pct), COLOR_HP);

        // ── MP bar indicator ────────────────────────────────────────────────────
        let mp_bar_w = ((s.mp_pct as i32) * 120 / 100).max(2);
        let mp_rect = RECT { left: 10, top: 30, right: 10 + mp_bar_w, bottom: 46 };
        let mp_brush = CreateSolidBrush(COLOR_MP) as HBRUSH;
        FillRect(hdc, &mp_rect, mp_brush);
        DeleteObject(mp_brush as _);
        draw_text_at(hdc, 135, 30, &format!("MP {}%", s.mp_pct), COLOR_MP);

        // ── Chicken threshold line ──────────────────────────────────────────────
        if s.chicken_hp_pct > 0 {
            let chick_x = 10 + (s.chicken_hp_pct as i32 * 120 / 100);
            let pen = CreatePen(PS_SOLID as i32, 1, COLOR_CHICK);
            let old_pen = SelectObject(hdc, pen as _);
            MoveToEx(hdc, chick_x, 8, std::ptr::null_mut());
            LineTo(hdc, chick_x, 28);
            SelectObject(hdc, old_pen);
            DeleteObject(pen as _);
        }

        // ── Nearest enemy crosshair ─────────────────────────────────────────────
        if s.enemy_count > 0 {
            let ex = s.nearest_enemy_x as i32;
            let ey = s.nearest_enemy_y as i32;
            let cr = 12i32; // crosshair radius
            let pen = CreatePen(PS_SOLID as i32, 2, COLOR_ENEMY);
            let old_pen = SelectObject(hdc, pen as _);
            MoveToEx(hdc, ex - cr, ey, std::ptr::null_mut());
            LineTo(hdc, ex + cr, ey);
            MoveToEx(hdc, ex, ey - cr, std::ptr::null_mut());
            LineTo(hdc, ex, ey + cr);
            // Enemy count label
            draw_text_at(hdc, ex + cr + 4, ey - 8, &format!("{}e {}%hp", s.enemy_count, s.nearest_enemy_hp_pct), COLOR_ENEMY);
            SelectObject(hdc, old_pen);
            DeleteObject(pen as _);
        }

        // ── Merc HP ────────────────────────────────────────────────────────────
        if s.merc_hp_pct < 100 {
            draw_text_at(hdc, 10, 50, &format!("Merc {}%", s.merc_hp_pct), COLOR_TEXT);
        }

        // ── Area name ──────────────────────────────────────────────────────────
        if !s.area_name.is_empty() {
            draw_text_at(hdc, 10, h - 20, &s.area_name, COLOR_TEXT);
        }
    }

    unsafe fn draw_standby_text(hdc: winapi::shared::windef::HDC, _w: i32, h: i32) {
        draw_text_at(hdc, 10, h - 20, "[KZB debug overlay — waiting for game]", COLOR_TEXT);
    }

    unsafe fn draw_text_at(hdc: winapi::shared::windef::HDC, x: i32, y: i32, text: &str, color: DWORD) {
        SetTextColor(hdc, color);
        SetBkMode(hdc, TRANSPARENT as i32);
        let wide: Vec<u16> = OsStr::new(text).encode_wide().collect();
        TextOutW(hdc, x, y, wide.as_ptr(), wide.len() as i32);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API — always compiled, delegates to win_impl on Windows, stubs on other
// ─────────────────────────────────────────────────────────────────────────────

pub struct OverlayWindow {
    #[cfg(windows)]
    inner: win_impl::OverlayWindow,
    #[cfg(not(windows))]
    _phantom: (),
}

impl OverlayWindow {
    pub fn create() -> Option<Self> {
        #[cfg(windows)]
        {
            win_impl::OverlayWindow::create().map(|inner| OverlayWindow { inner })
        }
        #[cfg(not(windows))]
        {
            eprintln!("[overlay] Debug overlay window is Windows-only — stub active");
            Some(OverlayWindow { _phantom: () })
        }
    }

    pub fn update(&self, state: DebugState) {
        #[cfg(windows)]
        self.inner.update(state);
        #[cfg(not(windows))]
        let _ = state;
    }

    pub fn destroy(self) {
        #[cfg(windows)]
        self.inner.destroy();
    }
}
