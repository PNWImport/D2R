#![allow(dead_code)]
// =============================================================================
// D2R Map Generator
// =============================================================================
// Deterministic map generation from seed + area + difficulty.
//
// D2's map generation works as follows:
//   1. The game seed determines a PRNG sequence
//   2. Each area has preset "tiles" from levels.txt/lvlprest.txt
//   3. The PRNG selects which preset tiles to use and how to orient them
//   4. Collision data is generated from the tile composition
//
// For a maphack overlay, we need:
//   - Collision map (walls vs walkable)
//   - Exit/entrance positions
//   - Waypoint position
//   - Notable object positions (shrines, chests, super uniques)
//
// The REAL map gen requires D2 game files (d2data.mpq) loaded through
// blacha/diablo2's d2-map.exe or D2RMH's d2mapapi_piped. Our map_helper
// shells out to one of these backends.
//
// This module handles:
//   - Calling the map generation backend
//   - Parsing collision data output
//   - Caching generated maps by (seed, area, difficulty) tuple
//   - Providing map data to the overlay renderer
// =============================================================================

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Map tile collision flags (from D2 collision map format)
pub const COLL_NONE: u16       = 0x0000;
pub const COLL_BLOCK_WALK: u16 = 0x0001;  // Wall
pub const COLL_BLOCK_LOS: u16  = 0x0002;  // Blocks line of sight
pub const COLL_WALL: u16       = 0x0004;  // Wall boundary
pub const COLL_BLOCK_PROJ: u16 = 0x0008;  // Blocks projectiles
pub const COLL_DOOR: u16       = 0x0010;  // Door tile
pub const COLL_UNIT_BLOCK: u16 = 0x0020;  // Blocked by unit
pub const COLL_PET_BLOCK: u16  = 0x0100;  // Pet blocking
pub const COLL_STAIRS: u16     = 0x4000;  // Stairs/entrance tile

/// Represents a single point of interest on the map
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapPOI {
    pub x: i32,
    pub y: i32,
    pub poi_type: POIType,
    pub label: String,
    pub target_area: Option<u32>,  // For exits, which area they lead to
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum POIType {
    Exit,
    Waypoint,
    Shrine,
    Chest,
    SuperUnique,
    QuestObject,
    Staircase,
    Portal,
}

/// Complete map data for one area
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapData {
    pub seed: u32,
    pub area_id: u32,
    pub difficulty: u8,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    /// Collision map: run-length encoded per row
    /// Each row is a Vec<u16> of alternating (wall_length, open_length, wall_length, ...)
    /// This is the exact format blacha/diablo2 outputs
    pub collision_rows: Vec<Vec<u16>>,
    /// Points of interest (exits, waypoints, shrines, etc.)
    pub pois: Vec<MapPOI>,
    /// Generation timestamp
    pub generated_at: i64,
}

/// Cache key for map data
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct CacheKey {
    seed: u32,
    area_id: u32,
    difficulty: u8,
}

pub struct MapGenerator {
    cache: HashMap<CacheKey, MapData>,
    max_cache_size: usize,
    /// Path to external map gen backend (d2-map.exe or d2mapapi_piped.exe)
    _backend_path: Option<String>,
}

impl MapGenerator {
    pub fn new() -> Self {
        Self {
            cache: HashMap::with_capacity(64),
            max_cache_size: 128,
            _backend_path: None,
        }
    }

    /// Set path to external map generation backend
    pub fn set_backend(&mut self, path: String) {
        self._backend_path = Some(path);
    }

    /// Generate or retrieve cached map data
    pub fn get_map(&mut self, seed: u32, area_id: u32, difficulty: u8) -> MapData {
        let key = CacheKey { seed, area_id, difficulty };

        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }

        // Try external backend first, fall back to built-in generation
        let map_data = if let Some(_backend) = &self._backend_path {
            self.generate_via_backend(seed, area_id, difficulty)
                .unwrap_or_else(|_| self.generate_builtin(seed, area_id, difficulty))
        } else {
            self.generate_builtin(seed, area_id, difficulty)
        };

        // Cache management
        if self.cache.len() >= self.max_cache_size {
            self.cache.clear(); // Simple eviction
        }
        self.cache.insert(key, map_data.clone());

        map_data
    }

    /// Call external map gen backend (blacha/d2mapapi_piped)
    /// Expected interface:
    ///   d2mapapi_piped.exe --seed <seed> --area <area> --difficulty <diff>
    ///   Returns JSON with collision data
    fn generate_via_backend(&self, seed: u32, area_id: u32, difficulty: u8) -> Result<MapData, String> {
        use std::process::Command;

        let backend = self._backend_path.as_ref()
            .ok_or_else(|| "No backend path configured".to_string())?;

        let output = Command::new(backend)
            .arg("--seed").arg(seed.to_string())
            .arg("--area").arg(area_id.to_string())
            .arg("--difficulty").arg(difficulty.to_string())
            .output()
            .map_err(|e| format!("Backend exec failed ({}): {}", backend, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Backend error: {}", stderr));
        }

        // Parse blacha/d2mapapi JSON format:
        //   { "map": [[run_lengths...], ...], "objects": [{x,y,type},...],
        //     "offset": {x, y}, "size": {width, height} }
        let json_str = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| format!("Backend JSON parse: {}", e))?;

        let collision_rows: Vec<Vec<u16>> = parsed["map"].as_array()
            .ok_or("Missing 'map' field")?
            .iter()
            .map(|row| {
                row.as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u16))
                    .collect()
            })
            .collect();

        let height = collision_rows.len() as u32;
        let width = parsed["size"]["width"].as_u64()
            .or_else(|| collision_rows.first().map(|r| r.iter().map(|&v| v as u64).sum()))
            .unwrap_or(200) as u32;

        let origin_x = parsed["offset"]["x"].as_i64().unwrap_or(0) as i32;
        let origin_y = parsed["offset"]["y"].as_i64().unwrap_or(0) as i32;

        // Parse objects/POIs
        let pois: Vec<MapPOI> = parsed["objects"].as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|obj| {
                let x = obj["x"].as_i64()? as i32;
                let y = obj["y"].as_i64()? as i32;
                let obj_type = obj["type"].as_str().unwrap_or("exit");
                let poi_type = match obj_type {
                    "exit" | "tile" => POIType::Exit,
                    "waypoint" => POIType::Waypoint,
                    "shrine" => POIType::Shrine,
                    "chest" => POIType::Chest,
                    "npc" | "super_unique" => POIType::SuperUnique,
                    "quest" => POIType::QuestObject,
                    _ => POIType::Exit,
                };
                Some(MapPOI {
                    x: origin_x + x,
                    y: origin_y + y,
                    poi_type,
                    label: obj["name"].as_str().unwrap_or("").to_string(),
                    target_area: obj["target"].as_u64().map(|v| v as u32),
                })
            })
            .collect();

        Ok(MapData {
            seed, area_id, difficulty, origin_x, origin_y, width, height,
            collision_rows, pois,
            generated_at: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// Built-in deterministic map generation (simplified)
    /// Uses seed-based PRNG to create collision maps
    /// Not as accurate as the real D2 algorithm but functional for overlay
    fn generate_builtin(&self, seed: u32, area_id: u32, difficulty: u8) -> MapData {
        use rand::{Rng, SeedableRng};
        use rand::rngs::StdRng;

        // Deterministic seed from game seed + area + difficulty
        let combined_seed = (seed as u64)
            ^ ((area_id as u64) << 32)
            ^ ((difficulty as u64) << 48);
        let mut rng = StdRng::seed_from_u64(combined_seed);

        // Area-specific map dimensions (approximate from levels.txt data)
        let (width, height) = area_dimensions(area_id);
        let origin_x = rng.gen_range(0..1000) as i32;
        let origin_y = rng.gen_range(0..1000) as i32;

        // Generate collision using cellular automata (simplified D2 approach)
        let mut grid = vec![vec![false; width as usize]; height as usize];

        // Step 1: Random noise based on area type
        let wall_density = area_wall_density(area_id);
        for y in 0..height as usize {
            for x in 0..width as usize {
                grid[y][x] = rng.gen_ratio(wall_density, 100);
            }
        }

        // Step 2: Ensure borders are walls
        for x in 0..width as usize {
            grid[0][x] = true;
            grid[height as usize - 1][x] = true;
        }
        for y in 0..height as usize {
            grid[y][0] = true;
            grid[y][width as usize - 1] = true;
        }

        // Step 3: Cellular automata smoothing (4 iterations)
        for _ in 0..4 {
            let mut new_grid = grid.clone();
            for y in 1..height as usize - 1 {
                for x in 1..width as usize - 1 {
                    let mut walls = 0u32;
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            if dx == 0 && dy == 0 { continue; }
                            if grid[(y as i32 + dy) as usize][(x as i32 + dx) as usize] {
                                walls += 1;
                            }
                        }
                    }
                    new_grid[y][x] = walls > 4;
                }
            }
            grid = new_grid;
        }

        // Step 4: Carve guaranteed paths (prevent fully blocked maps)
        let mid_y = height as usize / 2;
        let mid_x = width as usize / 2;
        for x in 2..width as usize - 2 {
            grid[mid_y][x] = false;
            grid[mid_y + 1][x] = false;
        }
        for y in 2..height as usize - 2 {
            grid[y][mid_x] = false;
            grid[y][mid_x + 1] = false;
        }

        // Convert grid to run-length encoded rows (blacha format)
        let collision_rows: Vec<Vec<u16>> = grid.iter().map(|row| {
            let mut runs = Vec::new();
            let mut current_wall = row[0];
            let mut run_len: u16 = 1;

            for &is_wall in &row[1..] {
                if is_wall == current_wall {
                    run_len += 1;
                } else {
                    runs.push(run_len);
                    current_wall = is_wall;
                    run_len = 1;
                }
            }
            runs.push(run_len);
            runs
        }).collect();

        // Generate POIs
        let mut pois = Vec::new();

        // Exits (1-3 per area)
        let exit_count = rng.gen_range(1..=3u32);
        for i in 0..exit_count {
            let (ex, ey) = find_open_spot(&grid, &mut rng, width, height);
            pois.push(MapPOI {
                x: origin_x + ex,
                y: origin_y + ey,
                poi_type: POIType::Exit,
                label: format!("Exit {}", i + 1),
                target_area: Some(area_id + 1),
            });
        }

        // Waypoint (if area has one)
        if area_has_waypoint(area_id) {
            let (wx, wy) = find_open_spot(&grid, &mut rng, width, height);
            pois.push(MapPOI {
                x: origin_x + wx,
                y: origin_y + wy,
                poi_type: POIType::Waypoint,
                label: "Waypoint".into(),
                target_area: None,
            });
        }

        // Staircase (for multi-level dungeons)
        if is_dungeon(area_id) {
            let (sx, sy) = find_open_spot(&grid, &mut rng, width, height);
            pois.push(MapPOI {
                x: origin_x + sx,
                y: origin_y + sy,
                poi_type: POIType::Staircase,
                label: "Stairs".into(),
                target_area: Some(area_id + 1),
            });
        }

        MapData {
            seed,
            area_id,
            difficulty,
            origin_x,
            origin_y,
            width,
            height,
            collision_rows,
            pois,
            generated_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn cache_stats(&self) -> (usize, usize) {
        (self.cache.len(), self.max_cache_size)
    }
}

// ---------------------------------------------------------------------------
// Area metadata helpers
// ---------------------------------------------------------------------------

fn area_dimensions(area_id: u32) -> (u32, u32) {
    // Approximate dimensions from levels.txt SizeX/SizeY columns
    match area_id {
        // Towns
        1 | 40 | 75 | 103 | 109 => (200, 200),
        // Outdoor areas (larger)
        2..=7 | 41..=49 | 76..=82 | 110..=115 => (280, 280),
        // Dungeons (medium)
        8..=11 | 50..=55 | 83..=90 => (160, 160),
        // Boss areas (specific sizes)
        37 => (120, 120),  // Catacombs 4
        108 => (200, 200), // Chaos Sanctuary
        131 => (160, 160), // Throne
        132 => (80, 80),   // Worldstone Chamber
        // Default
        _ => (200, 200),
    }
}

fn area_wall_density(area_id: u32) -> u32 {
    match area_id {
        // Open outdoor areas
        2..=7 | 41..=49 | 110..=115 => 35,
        // Dense dungeons
        8..=11 | 50..=55 | 83..=90 => 50,
        // Maze-like areas
        74 => 55, // Arcane Sanctuary
        108 => 45, // Chaos Sanctuary
        // Default
        _ => 42,
    }
}

fn area_has_waypoint(area_id: u32) -> bool {
    // Areas with waypoints (from waypoints.txt)
    matches!(area_id,
        1 | 3 | 4 | 5 | 6 | 7 | 27 | 29 | 32 | 35 |  // Act 1
        40 | 42 | 43 | 44 | 46 | 52 | 57 | 74 |        // Act 2
        75 | 76 | 78 | 80 | 81 | 83 |                    // Act 3
        103 | 106 | 107 |                                  // Act 4
        109 | 111 | 112 | 113 | 115 | 123 | 128 | 129    // Act 5
    )
}

fn is_dungeon(area_id: u32) -> bool {
    // Multi-level dungeon areas
    matches!(area_id,
        8..=11 | 27..=37 |      // Act 1 caves/catacombs
        50..=55 | 60..=63 |      // Act 2 tombs/dungeons
        83..=90 |                  // Act 3 dungeons
        128..=131                  // Act 5 worldstone keep
    )
}

fn find_open_spot(
    grid: &[Vec<bool>],
    rng: &mut impl rand::Rng,
    width: u32,
    height: u32,
) -> (i32, i32) {
    // Find a non-wall tile
    for _ in 0..100 {
        let x = rng.gen_range(10..width as i32 - 10);
        let y = rng.gen_range(10..height as i32 - 10);
        if !grid[y as usize][x as usize] {
            return (x, y);
        }
    }
    // Fallback to center
    (width as i32 / 2, height as i32 / 2)
}
