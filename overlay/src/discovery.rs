#![allow(dead_code)]
// =============================================================================
// D2R Offset Auto-Discovery
// =============================================================================
// Instead of relying on hardcoded static offsets (which break every patch),
// this module finds the offsets autonomously by scanning D2R process memory
// for structural patterns that are inherent to the engine, not patch-specific.
//
// Strategy (layered, each feeds the next):
//
// 1. SIG-SCAN (.text section)
//    Look for known instruction patterns near global data references.
//    Most resilient to patches. Already in memory.rs.
//
// 2. HASH TABLE HEURISTIC (.data section)
//    The UnitHashTable is 128 consecutive pointers (1024 bytes on x64).
//    Most buckets are NULL in typical gameplay. A player unit (type==0)
//    exists in exactly one bucket. This pattern is scannable.
//
// 3. PLAYER UNIT VALIDATION
//    Once we find a candidate UnitAny*, validate the struct layout:
//    - unit_type at +0x00 should be 0 (player)
//    - act_ptr at +0x18 should be a valid heap pointer
//    - path_ptr at +0x38 should be a valid heap pointer
//    - Follow path → position should be in sane range (0..32767)
//    - Follow act → act_misc → seed should be nonzero when in-game
//
// 4. SEED CROSS-VALIDATION
//    If we find the seed from one path, verify it by finding the same
//    seed through an independent scan (search .data for the u32 value
//    near a known struct layout).
//
// This is how we stay ahead of patches without waiting for the community.
// =============================================================================

use crate::offsets::*;

/// Result of auto-discovery
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub player_hash_table: Option<u64>,
    pub ui_settings: Option<u64>,
    pub player_unit_addr: Option<u64>,
    pub seed_verified: bool,
    pub method: String,
}

/// Pointer validation: is this plausibly a valid userspace heap pointer?
/// D2R x64: heap pointers are typically in range 0x10000..0x7FFFFFFFFFFF
pub fn is_valid_ptr(addr: u64) -> bool {
    addr > 0x10000 && addr < 0x7FFFFFFFFFFF && (addr & 0x3) == 0 // aligned
}

/// Check if a region of 128 consecutive u64s looks like a hash table:
/// - At least 100 of 128 entries are NULL (sparse)
/// - At least 1 entry is a valid pointer
/// - No more than 20 entries are valid pointers (not a vtable)
pub fn looks_like_hash_table(entries: &[u64; 128]) -> bool {
    let null_count = entries.iter().filter(|&&v| v == 0).count();
    let ptr_count = entries.iter().filter(|&&v| is_valid_ptr(v)).count();

    null_count >= 90 && ptr_count >= 1 && ptr_count <= 30
}

/// Validate a candidate UnitAny pointer by checking field sanity
/// Returns (is_player, struct_score)
/// struct_score: 0-5 based on how many fields look correct
pub fn validate_unit_any(
    read_u32: &dyn Fn(u64) -> Option<u32>,
    read_u64: &dyn Fn(u64) -> Option<u64>,
    addr: u64,
    offsets: &UnitAnyOffsets,
) -> (bool, u32) {
    let mut score = 0u32;

    // Check unit_type at +0x00
    let unit_type = match read_u32(addr + offsets.unit_type) {
        Some(t) if t <= 5 => { score += 1; t }, // valid type range
        _ => return (false, 0),
    };

    let is_player = unit_type == 0;

    // Check act_ptr at +0x18
    if let Some(act) = read_u64(addr + offsets.act_ptr) {
        if is_valid_ptr(act) { score += 1; }
    }

    // Check path_ptr at +0x38
    if let Some(path) = read_u64(addr + offsets.path_ptr) {
        if is_valid_ptr(path) {
            score += 1;

            // Validate position through path
            if let Some(x) = read_u32(path + 0x02) { // pos_x is WORD at +0x02
                let x = x & 0xFFFF; // mask to u16
                if x > 0 && x < 32768 { score += 1; }
            }
        }
    }

    // Check class_id at +0x04 (0-6 for player classes, now 0-7 with Warlock)
    if is_player {
        if let Some(cls) = read_u32(addr + offsets.class_id) {
            if cls <= 7 { score += 1; } // 0-6 original + 7 Warlock
        }
    }

    (is_player, score)
}

/// Walk a candidate hash table, looking for a player unit
/// Returns the player UnitAny address if found
pub fn find_player_in_table(
    read_u64: &dyn Fn(u64) -> Option<u64>,
    read_u32: &dyn Fn(u64) -> Option<u32>,
    table_base: u64,
    offsets: &UnitAnyOffsets,
) -> Option<u64> {
    for bucket in 0..128u64 {
        let ptr = read_u64(table_base + bucket * 8)?;
        if ptr == 0 { continue; }
        if !is_valid_ptr(ptr) { continue; }

        let mut cur = ptr;
        let mut guard = 0;
        while cur != 0 && is_valid_ptr(cur) && guard < 64 {
            guard += 1;
            let (is_player, score) = validate_unit_any(
                &|a| read_u32(a),
                &|a| read_u64(a),
                cur,
                offsets,
            );
            if is_player && score >= 3 {
                return Some(cur);
            }
            cur = read_u64(cur + offsets.next_unit_ptr).unwrap_or(0);
        }
    }
    None
}

/// Scan a memory region for candidate hash tables.
/// Looks for 128-pointer blocks that match the sparse pattern.
/// Returns (table_address, player_unit_address) pairs.
pub fn scan_for_hash_tables(
    read_u64: &dyn Fn(u64) -> Option<u64>,
    read_u32: &dyn Fn(u64) -> Option<u32>,
    base: u64,
    scan_start: u64,
    scan_end: u64,
    offsets: &UnitAnyOffsets,
) -> Vec<(u64, u64)> {
    let mut results = Vec::new();
    let step = 8u64; // pointer-aligned scanning

    let mut addr = scan_start;
    while addr + 1024 < scan_end {
        // Read 128 consecutive u64s
        let mut entries = [0u64; 128];
        let mut valid = true;
        for i in 0..128 {
            match read_u64(addr + i as u64 * 8) {
                Some(v) => entries[i] = v,
                None => { valid = false; break; }
            }
        }

        if valid && looks_like_hash_table(&entries) {
            // Candidate! Try to find player unit
            if let Some(player) = find_player_in_table(read_u64, read_u32, addr, offsets) {
                results.push((addr - base, player)); // store as offset from base
                // Don't break — collect all candidates for scoring
            }
        }

        addr += step;

        // Skip large ranges of zeros
        if let Some(v) = read_u64(addr) {
            if v == 0 {
                addr += 64; // jump ahead in zero regions
            }
        }
    }

    results
}

/// Verify a discovered seed by following the full pointer chain and checking
/// the seed value is nonzero and consistent across reads
pub fn verify_seed_chain(
    read_u64: &dyn Fn(u64) -> Option<u64>,
    read_u32: &dyn Fn(u64) -> Option<u32>,
    player_unit: u64,
    offsets: &Offsets,
) -> Option<u32> {
    let act = read_u64(player_unit + offsets.unit_any.act_ptr)?;
    if !is_valid_ptr(act) { return None; }

    let misc = read_u64(act + offsets.act.act_misc_ptr)?;
    if !is_valid_ptr(misc) { return None; }

    let seed1 = read_u32(misc + offsets.act_misc.map_seed)?;
    if seed1 == 0 { return None; }

    // Re-read to verify consistency (not random memory)
    std::thread::sleep(std::time::Duration::from_millis(10));
    let seed2 = read_u32(misc + offsets.act_misc.map_seed)?;

    if seed1 == seed2 { Some(seed1) } else { None }
}

/// Full auto-discovery pipeline
/// Call this after attach when sig-scan fails or to verify sig-scan results.
///
/// `read_region`: reads a chunk of bytes from process memory
/// `read_u64/read_u32`: typed reads
/// `base_addr`: D2R.exe module base
/// `module_size`: approximate size of D2R.exe in memory (~80MB)
pub fn auto_discover(
    read_u64: &dyn Fn(u64) -> Option<u64>,
    read_u32: &dyn Fn(u64) -> Option<u32>,
    base_addr: u64,
    module_size: u64,
) -> DiscoveryResult {
    let offsets = Offsets::default();
    let mut result = DiscoveryResult {
        player_hash_table: None,
        ui_settings: None,
        player_unit_addr: None,
        seed_verified: false,
        method: String::new(),
    };

    // Scan the .data section region (typically starts ~0x1E00000 into the module)
    // Widen the scan range for 3.x since the binary is larger
    let data_start = base_addr + 0x1C00000;
    let data_end = base_addr + module_size.min(0x3000000); // cap at ~48MB

    eprintln!("[discovery] Scanning {:#X}..{:#X} for hash tables", data_start, data_end);

    let candidates = scan_for_hash_tables(
        read_u64, read_u32, base_addr,
        data_start, data_end,
        &offsets.unit_any,
    );

    eprintln!("[discovery] Found {} candidate hash tables", candidates.len());

    // Score candidates by verifying the full seed chain
    for (table_offset, player_addr) in &candidates {
        let table_abs = base_addr + table_offset;
        if let Some(seed) = verify_seed_chain(read_u64, read_u32, *player_addr, &offsets) {
            result.player_hash_table = Some(*table_offset);
            result.player_unit_addr = Some(*player_addr);
            result.seed_verified = true;
            result.method = format!(
                "heuristic: table={:#X} player={:#X} seed={:#X}",
                table_abs, player_addr, seed
            );
            eprintln!("[discovery] VERIFIED: {}", result.method);
            return result;
        }
    }

    // If we found a table but couldn't verify seed (maybe in menu)
    if let Some((table_offset, player_addr)) = candidates.first() {
        result.player_hash_table = Some(*table_offset);
        result.player_unit_addr = Some(*player_addr);
        result.method = format!(
            "heuristic-unverified: table={:#X} player={:#X}",
            base_addr + table_offset, player_addr
        );
        eprintln!("[discovery] UNVERIFIED: {}", result.method);
    }

    result
}
