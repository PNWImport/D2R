// =============================================================================
// Game Process Memory Reader - FULL WINDOWS PRODUCTION + DEV FALLBACK
// =============================================================================
// cfg(windows) = real ReadProcessMemory, sig-scan, the works
// cfg(not(windows)) = simulated state for dev/test (this Linux box)
//
// Pointer chains:
//   Seed:     PlayerUnit → pAct(+0x20) → pActMisc(+0x78) → dwMapSeed(+0x840)
//   Diff:     ... → pActMisc(+0x78) → dwDifficulty(+0x830)
//   Position: PlayerUnit → pPath(+0x38) → xPos(+0x02), yPos(+0x06)
//   Area:     pPath → pRoom1(+0x20) → pRoom2(+0x18) → pLevel(+0x90) → dwLevelNo(+0x1F8)
// =============================================================================

use crate::offsets::*;
use serde::{Deserialize, Serialize};

/// Build the target process name at runtime to avoid string literal in binary.
/// Returns the game process name without it appearing as a contiguous string in .rdata.
#[allow(dead_code)]
fn target_process_name() -> String {
    // XOR-obfuscated bytes for the process name
    // Each byte is XOR'd with 0x5A
    const OBFUSCATED: [u8; 7] = [
        b'D' ^ 0x5A,  // 0x1E
        b'2' ^ 0x5A,  // 0x68
        b'R' ^ 0x5A,  // 0x08
        b'.' ^ 0x5A,  // 0x74
        b'e' ^ 0x5A,  // 0x3F
        b'x' ^ 0x5A,  // 0x22
        b'e' ^ 0x5A,  // 0x3F
    ];
    OBFUSCATED.iter().map(|b| (b ^ 0x5A) as char).collect()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameState {
    pub in_game: bool,
    pub map_seed: u32,
    pub difficulty: u8,
    pub area_id: u32,
    pub area_name: String,
    pub act: u8,
    pub player_x: u16,
    pub player_y: u16,
    pub player_name: String,
    pub is_town: bool,
    pub timestamp_ms: i64,
}

/// Humanized timing jitter
pub fn jitter_delay_ms() -> u64 {
    use rand::Rng;
    rand::thread_rng().gen_range(25..75)
}

// =====================================================================
// WINDOWS: Real implementation
// =====================================================================
#[cfg(windows)]
mod platform {
    use super::*;
    use std::{mem, ptr};
    use winapi::shared::minwindef::{DWORD, FALSE, HMODULE};
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::memoryapi::ReadProcessMemory;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::psapi::EnumProcessModulesEx;
    use winapi::um::synchapi::WaitForSingleObject;
    use winapi::um::tlhelp32::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
        PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use winapi::um::winnt::{HANDLE, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

    const LIST_MODULES_64BIT: DWORD = 0x02;

    pub struct ProcessReader {
        offsets: Offsets,
        handle: HANDLE,
        base: u64,
        pid: u32,
        attached: bool,
        sig_done: bool,
    }

    impl ProcessReader {
        pub fn new() -> Self {
            let mut offsets = Offsets::default();
            offsets.load_overrides();
            Self {
                offsets,
                handle: ptr::null_mut(),
                base: 0,
                pid: 0,
                attached: false,
                sig_done: false,
            }
        }

        pub fn is_attached(&self) -> bool { self.attached }

        pub fn attach(&mut self) -> Result<(), String> {
            if self.attached && self.process_alive() {
                return Ok(());
            }
            self.detach();

            unsafe {
                let target = super::target_process_name();
                let pid = find_pid(&target)?;
                let h = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
                if h.is_null() {
                    return Err(format!("OpenProcess PID {}: err {}", pid, GetLastError()));
                }

                self.handle = h;
                self.pid = pid;
                self.base = module_base(h)?;

                if !self.sig_done {
                    if let Err(e) = self.run_sig_scan() {
                        eprintln!("[map] sig-scan failed (trying auto-discovery): {}", e);
                        // Fallback: heuristic auto-discovery
                        self.run_auto_discovery();
                    } else {
                        self.sig_done = true;
                    }
                }

                self.attached = true;
                eprintln!("[map] attached pid={} base={:#X}", pid, self.base);
                Ok(())
            }
        }

        fn detach(&mut self) {
            if !self.handle.is_null() {
                unsafe { CloseHandle(self.handle); }
            }
            self.handle = ptr::null_mut();
            self.attached = false;
            self.pid = 0;
            self.base = 0;
        }

        fn process_alive(&self) -> bool {
            unsafe { WaitForSingleObject(self.handle, 0) == 258 } // WAIT_TIMEOUT
        }

        // ---- RPM wrappers ----

        fn rpm<T: Copy + Default>(&self, addr: u64) -> Result<T, String> {
            let mut val: T = Default::default();
            let size = mem::size_of::<T>();
            let ok = unsafe {
                ReadProcessMemory(
                    self.handle,
                    addr as *const _,
                    &mut val as *mut T as *mut _,
                    size,
                    ptr::null_mut(),
                )
            };
            if ok == FALSE {
                Err(format!("RPM {:#X} ({}B): err {}", addr, size, unsafe { GetLastError() }))
            } else {
                Ok(val)
            }
        }

        #[allow(dead_code)]
        fn read_u8(&self, a: u64)  -> Result<u8, String>  { self.rpm::<u8>(a) }
        fn read_u16(&self, a: u64) -> Result<u16, String> { self.rpm::<u16>(a) }
        fn read_u32(&self, a: u64) -> Result<u32, String> { self.rpm::<u32>(a) }
        fn read_u64(&self, a: u64) -> Result<u64, String> { self.rpm::<u64>(a) }

        fn read_bytes(&self, addr: u64, buf: &mut [u8]) -> Result<usize, String> {
            let mut read: usize = 0;
            let ok = unsafe {
                ReadProcessMemory(
                    self.handle,
                    addr as *const _,
                    buf.as_mut_ptr() as *mut _,
                    buf.len(),
                    &mut read as *mut usize as *mut _,
                )
            };
            if ok == FALSE {
                Err(format!("RPM bytes {:#X}: err {}", addr, unsafe { GetLastError() }))
            } else {
                Ok(read)
            }
        }

        // ---- Player unit lookup ----

        fn find_player_unit(&self) -> Result<u64, String> {
            let table = self.base + self.offsets.player_hash_table;
            for bucket in 0..128u64 {
                let ptr = match self.read_u64(table + bucket * 8) {
                    Ok(v) if v != 0 => v,
                    _ => continue,
                };

                let mut cur = ptr;
                let mut guard = 0u32;
                while cur != 0 && guard < 512 {
                    guard += 1;
                    if let Ok(utype) = self.read_u32(cur + self.offsets.unit_any.unit_type) {
                        if utype == 0 { return Ok(cur); } // 0 = Player
                    }
                    cur = self.read_u64(cur + self.offsets.unit_any.next_unit_ptr).unwrap_or(0);
                }
            }
            Err("Player unit not found (menu/loading?)".into())
        }

        fn read_area_id(&self, path_ptr: u64) -> Result<u32, String> {
            let room1 = self.read_u64(path_ptr + self.offsets.path.room_ptr)?;
            if room1 == 0 { return Err("Room1 null".into()); }
            let room2 = self.read_u64(room1 + self.offsets.room1.room_ex_ptr)?;
            if room2 == 0 { return Err("Room2 null".into()); }
            let level = self.read_u64(room2 + self.offsets.room2.level_ptr)?;
            if level == 0 { return Err("Level null".into()); }
            self.read_u32(level + self.offsets.level.level_id)
        }

        fn read_player_name(&self, unit: u64) -> String {
            // PlayerData at UnitAny.union_ptr (+0x10), name is first field (wchar[16])
            let pdata = self.read_u64(unit + self.offsets.unit_any.union_ptr).unwrap_or(0);
            if pdata == 0 { return String::new(); }
            let mut buf = [0u8; 32];
            if self.read_bytes(pdata, &mut buf).is_err() { return String::new(); }
            // Name is stored as narrow char[] in D2R
            buf.iter()
                .take_while(|&&b| b != 0 && b.is_ascii())
                .map(|&b| b as char)
                .collect()
        }

        // ---- Sig scan ----

        fn run_sig_scan(&mut self) -> Result<(), String> {
            let scan_size: usize = 0x2200000; // ~34MB
            let mut buf = vec![0u8; scan_size];
            let read = self.read_bytes(self.base, &mut buf)?;
            let data = &buf[..read];

            // (sig, is_critical)
            // Critical = must be resolved for any reading to work.
            // Non-critical = useful but not required.
            let sigs: &[(&SigPattern, bool)] = &[
                (&SIG_UNIT_HASH_TABLE, true),
                (&SIG_UI_SETTINGS,     true),
                (&SIG_EXPANSION,       false),
                (&SIG_ROSTER_DATA,     false),
                (&SIG_GAME_INFO,       false),
                (&SIG_MAP_SEED,        false),
            ];

            let mut ok_count = 0u32;
            let mut report  = Vec::<String>::new();

            for &(sig, critical) in sigs {
                // Skip patterns disabled by offsets.json
                if self.offsets.disabled_sigs.iter().any(|n| n == sig.name) {
                    report.push(format!("  {:20} DISABLED (skipped per offsets.json)", sig.name));
                    continue;
                }

                let (first, count) = sig_find_unique(data, sig);
                match (first, count) {
                    (Some(off), 1) => {
                        let resolved = resolve_sig(data, self.base, off, sig);
                        // Wire critical results into offsets
                        match sig.name {
                            "UnitHashTable" => {
                                self.offsets.player_hash_table = resolved - self.base;
                            }
                            "UISettings" => {
                                self.offsets.ui_settings = resolved - self.base;
                            }
                            _ => {}
                        }
                        report.push(format!("  {:20} OK  -> {:#X}", sig.name, resolved));
                        ok_count += 1;
                    }
                    (None, _) => {
                        let flag = if critical { "FAILED (CRITICAL)" } else { "FAILED" };
                        report.push(format!("  {:20} {}", sig.name, flag));
                    }
                    (Some(off), n) => {
                        // Ambiguous: log loudly, still use first match
                        let resolved = resolve_sig(data, self.base, off, sig);
                        match sig.name {
                            "UnitHashTable" => self.offsets.player_hash_table = resolved - self.base,
                            "UISettings"    => self.offsets.ui_settings = resolved - self.base,
                            _ => {}
                        }
                        report.push(format!("  {:20} AMBIGUOUS ({} matches) — using first -> {:#X}",
                            sig.name, n, resolved));
                        ok_count += 1;
                    }
                }
            }

            // Runtime sanity check on the hash-table pointer: read the first bucket
            // and verify it looks like a plausible process pointer (non-zero, user-space).
            if self.offsets.player_hash_table != 0 {
                let table_va = self.base + self.offsets.player_hash_table;
                match self.read_u64(table_va) {
                    Ok(first_bucket) if first_bucket == 0 => {
                        // Bucket 0 null is normal (empty), scan a few more
                        let any_nonnull = (1..16u64).any(|i| {
                            self.read_u64(table_va + i * 8).ok().map_or(false, |v| v != 0)
                        });
                        if !any_nonnull {
                            report.push(format!("  {:20} WARNING: all first 16 buckets null — wrong address or not in-game",
                                "UnitHashTable"));
                        }
                    }
                    Ok(first_bucket) => {
                        // Non-null: check pointer looks like a user-space VA
                        let plausible = first_bucket > 0x10000 && first_bucket < 0x0008_0000_0000_0000;
                        if !plausible {
                            report.push(format!("  {:20} WARNING: bucket[0]={:#X} looks invalid (kernel/zero VA)",
                                "UnitHashTable", first_bucket));
                        }
                    }
                    Err(e) => {
                        report.push(format!("  {:20} WARNING: sanity RPM failed: {}", "UnitHashTable", e));
                    }
                }
            }

            eprintln!("[map] sig-scan report ({}/{} matched):", ok_count, sigs.len());
            for line in &report { eprintln!("[map]{}", line); }

            // Validate that critical fields were resolved
            if let Err(e) = self.offsets.validate() {
                eprintln!("[map] CRITICAL: {}", e);
                return Err(e);
            }
            Ok(())
        }

        // ---- Full game state read ----

        /// Run heuristic auto-discovery when sig-scan fails
        fn run_auto_discovery(&mut self) {
            use crate::discovery;

            let base = self.base;
            let handle = self.handle;

            // Create closures that capture the RPM handle
            let read_u64_fn = |addr: u64| -> Option<u64> {
                let mut val: u64 = 0;
                let ok = unsafe {
                    ReadProcessMemory(
                        handle, addr as *const _, &mut val as *mut u64 as *mut _,
                        8, ptr::null_mut(),
                    )
                };
                if ok == FALSE { None } else { Some(val) }
            };

            let read_u32_fn = |addr: u64| -> Option<u32> {
                let mut val: u32 = 0;
                let ok = unsafe {
                    ReadProcessMemory(
                        handle, addr as *const _, &mut val as *mut u32 as *mut _,
                        4, ptr::null_mut(),
                    )
                };
                if ok == FALSE { None } else { Some(val) }
            };

            let result = discovery::auto_discover(
                &read_u64_fn, &read_u32_fn,
                base, 0x3000000, // ~48MB scan range
            );

            if let Some(offset) = result.player_hash_table {
                self.offsets.player_hash_table = offset;
                self.sig_done = true;
                eprintln!("[map] Auto-discovery: hash_table={:#X} verified={}",
                    base + offset, result.seed_verified);
            } else {
                eprintln!("[map] Auto-discovery: failed, falling back to static offsets");
            }
        }

        pub fn read_game_state(&self) -> Result<GameState, String> {
            if !self.attached { return Err("Not attached".into()); }
            let now = chrono::Utc::now().timestamp_millis();

            let player = self.find_player_unit()?;

            // Act → ActMisc → seed & difficulty
            let act_ptr = self.read_u64(player + self.offsets.unit_any.act_ptr)?;
            if act_ptr == 0 { return Err("Act null".into()); }
            let act_id = self.read_u32(act_ptr + self.offsets.act.act_id)?;

            let misc = self.read_u64(act_ptr + self.offsets.act.act_misc_ptr)?;
            if misc == 0 { return Err("ActMisc null".into()); }
            let map_seed = self.read_u32(misc + self.offsets.act_misc.map_seed)?;
            let difficulty = self.read_u32(misc + self.offsets.act_misc.difficulty)? as u8;

            // Path → position
            let path = self.read_u64(player + self.offsets.unit_any.path_ptr)?;
            if path == 0 { return Err("Path null".into()); }
            let pos_x = self.read_u16(path + self.offsets.path.pos_x)?;
            let pos_y = self.read_u16(path + self.offsets.path.pos_y)?;

            // Area
            let area_id = self.read_area_id(path).unwrap_or(0);
            let area = AreaId::from_u32(area_id);

            let name = self.read_player_name(player);

            std::thread::sleep(std::time::Duration::from_millis(jitter_delay_ms()));

            Ok(GameState {
                in_game: true,
                map_seed,
                difficulty,
                area_id,
                area_name: area.name().to_string(),
                act: (act_id + 1).min(5) as u8,
                player_x: pos_x,
                player_y: pos_y,
                player_name: if name.is_empty() { "Player".into() } else { name },
                is_town: area.is_town(),
                timestamp_ms: now,
            })
        }
    }

    impl Drop for ProcessReader {
        fn drop(&mut self) { self.detach(); }
    }

    // ---- Helpers ----

    unsafe fn find_pid(name: &str) -> Result<u32, String> {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap == INVALID_HANDLE_VALUE {
            return Err(format!("Snapshot failed: {}", GetLastError()));
        }
        let mut entry: PROCESSENTRY32W = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) == FALSE {
            CloseHandle(snap);
            return Err("Process32First failed".into());
        }
        loop {
            let exe: String = entry.szExeFile.iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8 as char)
                .collect();
            if exe.eq_ignore_ascii_case(name) {
                let pid = entry.th32ProcessID;
                CloseHandle(snap);
                return Ok(pid);
            }
            if Process32NextW(snap, &mut entry) == FALSE { break; }
        }
        CloseHandle(snap);
        Err(format!("{} not found", name))
    }

    unsafe fn module_base(h: HANDLE) -> Result<u64, String> {
        let mut module: HMODULE = ptr::null_mut();
        let mut needed: DWORD = 0;
        let ok = EnumProcessModulesEx(
            h, &mut module, mem::size_of::<HMODULE>() as u32,
            &mut needed, LIST_MODULES_64BIT,
        );
        if ok == FALSE {
            return Err(format!("EnumProcessModulesEx: err {}", GetLastError()));
        }
        Ok(module as u64)
    }

    /// Scan `buf` for all occurrences of `sig`.
    /// Returns (first_match_offset, total_match_count).
    /// A count > 1 means the pattern is ambiguous — caller should log a warning.
    fn sig_find_unique(buf: &[u8], sig: &SigPattern) -> (Option<usize>, usize) {
        let pat  = sig.pattern;
        let mask = sig.mask.as_bytes();
        if buf.len() < pat.len() { return (None, 0); }
        let mut first: Option<usize> = None;
        let mut count = 0usize;
        'outer: for i in 0..buf.len() - pat.len() {
            for j in 0..pat.len() {
                if mask[j] == b'x' && buf[i + j] != pat[j] { continue 'outer; }
            }
            if first.is_none() { first = Some(i); }
            count += 1;
        }
        (first, count)
    }

    fn resolve_sig(buf: &[u8], scan_base: u64, match_offset: usize, sig: &SigPattern) -> u64 {
        // addr_offset is signed (i64) — the i32 displacement can be before the match
        let o = (match_offset as i64 + sig.addr_offset) as usize;
        let rel = i32::from_le_bytes([buf[o], buf[o+1], buf[o+2], buf[o+3]]);

        match sig.mode {
            ResolveMode::RipRelative => {
                // Standard x64 RIP-relative: displacement is relative to the byte
                // AFTER the 4-byte immediate (instruction end for the imm field).
                let rip = scan_base + (o + sig.addr_size) as u64;
                (rip as i64 + rel as i64 + sig.extra_offset) as u64
            }
            ResolveMode::BaseRelative => {
                // Value is an RVA from module base.
                (scan_base as i64 + rel as i64 + sig.extra_offset) as u64
            }
        }
    }
}

// =====================================================================
// NON-WINDOWS: Dev/test simulation (this Linux build)
// =====================================================================
#[cfg(not(windows))]
mod platform {
    use super::*;

    #[allow(dead_code)]
    pub struct ProcessReader {
        offsets: Offsets,
        attached: bool,
    }

    impl ProcessReader {
        pub fn new() -> Self {
            let mut offsets = Offsets::default();
            offsets.load_overrides();
            Self { offsets, attached: false }
        }
        pub fn is_attached(&self) -> bool { self.attached }
        pub fn attach(&mut self) -> Result<(), String> {
            self.attached = true;
            Ok(())
        }
        pub fn read_game_state(&self) -> Result<GameState, String> {
            if !self.attached { return Err("Not attached".into()); }
            let now = chrono::Utc::now().timestamp_millis();
            Ok(GameState {
                in_game: true,
                map_seed: 0xDEADBEEF,
                difficulty: 2,
                area_id: 131,
                area_name: AreaId::from_u32(131).name().to_string(),
                act: 5, player_x: 5085, player_y: 5040,
                player_name: "TestSorc".into(),
                is_town: false, timestamp_ms: now,
            })
        }
    }
}

// Re-export the platform-specific ProcessReader
pub use platform::ProcessReader;
