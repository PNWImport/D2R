#![allow(dead_code)]
// =============================================================================
// Chrome Native Messaging Host Protocol
// =============================================================================
// Same architecture as the vision agent chrome_helper.exe:
//   - Runs as a Chrome Native Messaging Host
//   - Communicates via stdin/stdout with length-prefixed JSON
//   - Registered as "com.d2vision.map" (separate from "com.d2vision.agent")
//   - Disguised as Chrome component process
//
// Wire format (Chrome Native Messaging):
//   [4 bytes LE length][JSON payload]
// =============================================================================

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read, Write};

// ---------------------------------------------------------------------------
// Command types from extension -> map_helper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum InboundCommand {
    #[serde(rename = "handshake")]
    Handshake { version: Option<String> },

    #[serde(rename = "ping")]
    Ping { timestamp: Option<i64> },

    #[serde(rename = "toggle_map")]
    ToggleMap { enabled: bool },

    #[serde(rename = "read_state")]
    ReadState,

    #[serde(rename = "generate_map")]
    GenerateMap {
        seed: u32,
        area_id: u32,
        difficulty: u8,
    },

    #[serde(rename = "set_opacity")]
    SetOpacity { opacity: u8 },

    #[serde(rename = "set_area")]
    SetArea { area: u32, difficulty: u8 },

    #[serde(rename = "set_backend")]
    SetBackend { path: String },

    #[serde(rename = "cache_stats")]
    CacheStats,

    #[serde(rename = "get_offsets")]
    GetOffsets,

    #[serde(rename = "shutdown")]
    Shutdown,
}

// ---------------------------------------------------------------------------
// Response types from map_helper -> extension
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub cmd: String,
    #[serde(flatten)]
    pub data: Value,
}

// ---------------------------------------------------------------------------
// Native Messaging I/O
// ---------------------------------------------------------------------------

/// Read one native message from stdin (blocking)
pub fn read_message() -> io::Result<Option<Value>> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    // Read 4-byte length prefix (little-endian)
    let mut len_buf = [0u8; 4];
    match handle.read_exact(&mut len_buf) {
        Ok(()) => {},
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let msg_len = u32::from_le_bytes(len_buf) as usize;

    // Sanity check (Chrome limits to 1MB)
    if msg_len > 1_048_576 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Message too large: {} bytes", msg_len),
        ));
    }

    // Read message body
    let mut msg_buf = vec![0u8; msg_len];
    handle.read_exact(&mut msg_buf)?;

    // Parse JSON
    let value: Value = serde_json::from_slice(&msg_buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(Some(value))
}

/// Write one native message to stdout
pub fn send_message(msg: &Value) -> io::Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    let msg_bytes = serde_json::to_vec(msg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let len = msg_bytes.len() as u32;
    handle.write_all(&len.to_le_bytes())?;
    handle.write_all(&msg_bytes)?;
    handle.flush()?;

    Ok(())
}

/// Send a typed response
pub fn send_response(cmd: &str, data: Value) -> io::Result<()> {
    let mut msg = json!({ "cmd": cmd });
    if let Value::Object(map) = data {
        for (k, v) in map {
            msg[k] = v;
        }
    }
    send_message(&msg)
}

/// Send an error response
pub fn send_error(context: &str, error: &str) -> io::Result<()> {
    send_message(&json!({
        "cmd": "error",
        "context": context,
        "error": error,
        "timestamp": chrono::Utc::now().timestamp_millis(),
    }))
}

/// Parse an inbound command from raw JSON
pub fn parse_command(value: &Value) -> Result<InboundCommand, String> {
    // First try structured deserialization
    if let Ok(cmd) = serde_json::from_value::<InboundCommand>(value.clone()) {
        return Ok(cmd);
    }

    // Fallback: manual parsing for flexibility
    match value["cmd"].as_str() {
        Some("handshake") => Ok(InboundCommand::Handshake {
            version: value["version"].as_str().map(|s| s.to_string()),
        }),
        Some("ping") => Ok(InboundCommand::Ping {
            timestamp: value["timestamp"].as_i64(),
        }),
        Some("toggle_map") => Ok(InboundCommand::ToggleMap {
            enabled: value["enabled"].as_bool().unwrap_or(true),
        }),
        Some("read_state") => Ok(InboundCommand::ReadState),
        Some("generate_map") => Ok(InboundCommand::GenerateMap {
            seed: value["seed"].as_u64().unwrap_or(0) as u32,
            area_id: value["area_id"].as_u64().unwrap_or(0) as u32,
            difficulty: value["difficulty"].as_u64().unwrap_or(0) as u8,
        }),
        Some("set_opacity") => Ok(InboundCommand::SetOpacity {
            opacity: value["opacity"].as_u64().unwrap_or(180) as u8,
        }),
        Some("set_area") => Ok(InboundCommand::SetArea {
            area: value["area"].as_u64().unwrap_or(1) as u32,
            difficulty: value["difficulty"].as_u64().unwrap_or(0) as u8,
        }),
        Some("set_backend") => Ok(InboundCommand::SetBackend {
            path: value["path"].as_str().unwrap_or("").to_string(),
        }),
        Some("cache_stats") => Ok(InboundCommand::CacheStats),
        Some("get_offsets") => Ok(InboundCommand::GetOffsets),
        Some("shutdown") => Ok(InboundCommand::Shutdown),
        _ => Err(format!("Unknown command: {:?}", value["cmd"])),
    }
}
