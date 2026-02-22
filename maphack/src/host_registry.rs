//! Dynamic Host Name Registry
//!
//! Generates and manages random 12-16 character hex host names per startup.
//! Names are stored in the KillZBot config directory and used for native messaging.

use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

const CONFIG_DIR: &str = "KillZBot";
const HOST_NAME_MIN: usize = 12;
const HOST_NAME_MAX: usize = 16;

#[derive(Debug, Serialize, Deserialize)]
pub struct HostRegistry {
    pub maphack_host: String,
}

impl HostRegistry {
    /// Load or create the host registry from config directory
    pub fn load_or_create() -> Result<Self, String> {
        let config_path = Self::config_file_path()?;

        // Try to load existing registry
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(registry) = serde_json::from_str::<HostRegistry>(&content) {
                    eprintln!("[host_registry] Loaded existing hosts from {:?}", config_path);
                    return Ok(registry);
                }
            }
        }

        // Generate new random host name
        let maphack_host = Self::generate_random_host();
        let registry = HostRegistry {
            maphack_host: maphack_host.clone(),
        };

        // Save to config file
        if let Err(e) = registry.save(&config_path) {
            eprintln!("[host_registry] Failed to save registry: {}", e);
            // Continue anyway with in-memory registry
        } else {
            eprintln!("[host_registry] Created new hosts at {:?}", config_path);
        }

        Ok(registry)
    }

    /// Generate a random 12-16 character hex string
    fn generate_random_host() -> String {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let len = rng.gen_range(HOST_NAME_MIN..=HOST_NAME_MAX);
        let bytes: Vec<u8> = (0..len)
            .map(|_| {
                let val = rng.gen_range(0..16);
                if val < 10 {
                    b'0' + val as u8
                } else {
                    b'a' + (val - 10) as u8
                }
            })
            .collect();

        String::from_utf8(bytes).unwrap_or_else(|_| "chromium".to_string())
    }

    /// Get the config directory, creating it if necessary
    fn config_dir() -> Result<PathBuf, String> {
        let config_base = if cfg!(windows) {
            PathBuf::from(
                std::env::var("PROGRAMDATA")
                    .unwrap_or_else(|_| "C:\\ProgramData".to_string()),
            )
        } else {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string()))
                .join(".config")
        };

        let config_path = config_base.join(CONFIG_DIR);

        if !config_path.exists() {
            fs::create_dir_all(&config_path)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }

        Ok(config_path)
    }

    /// Get the full path to the host registry config file
    fn config_file_path() -> Result<PathBuf, String> {
        let dir = Self::config_dir()?;
        Ok(dir.join("hosts.json"))
    }

    /// Save the registry to disk
    fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize registry: {}", e))?;
        fs::write(path, json).map_err(|e| format!("Failed to write registry: {}", e))?;
        Ok(())
    }

    /// Get the maphack host name with "com.chromium." prefix
    pub fn maphack_host_name(&self) -> String {
        format!("com.chromium.{}", self.maphack_host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_random_host() {
        let host = HostRegistry::generate_random_host();
        assert!((HOST_NAME_MIN..=HOST_NAME_MAX).contains(&host.len()));
        assert!(host.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_host_name_format() {
        let registry = HostRegistry {
            maphack_host: "abcd1234".to_string(),
        };
        assert_eq!(registry.maphack_host_name(), "com.chromium.abcd1234");
    }
}
