//! Chrome Native Messaging Host
//!
//! Implements Chrome's native messaging stdio protocol to make the agent
//! a legitimate Chrome child process. Chrome spawns us, we inherit the
//! process tree — no PEB manipulation or hooks needed.
//!
//! Protocol: 4-byte LE length prefix + JSON payload, both directions.
//!
//! # Stealth Fixes (vs uploaded reference)
//! - No chrome.notifications (visible toast = stealth leak)
//! - No chrome.storage logging (forensic evidence)
//! - No badge text updates (visible indicator)
//! - Graceful shutdown with cleanup window (not instant exit)
//! - Generic naming: "Display Calibration Service" not "D2 Vision"

use crate::vision::ShardedFrameBuffer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

/// Stats shared between the agent and the native messaging host.
/// All fields are atomic — safe to read from any thread.
#[derive(Debug)]
pub struct SharedAgentStats {
    pub frames_processed: AtomicU64,
    pub decisions_made: AtomicU64,
    pub kills: AtomicU64,
    pub deaths: AtomicU64,
    pub potions_used: AtomicU64,
    pub loots_picked: AtomicU64,
    pub chickens: AtomicU64,
    pub session_start: Instant,
    pub paused: AtomicBool,
    pub config_version: AtomicU64,
}

impl SharedAgentStats {
    pub fn new() -> Self {
        Self {
            frames_processed: AtomicU64::new(0),
            decisions_made: AtomicU64::new(0),
            kills: AtomicU64::new(0),
            deaths: AtomicU64::new(0),
            potions_used: AtomicU64::new(0),
            loots_picked: AtomicU64::new(0),
            chickens: AtomicU64::new(0),
            session_start: Instant::now(),
            paused: AtomicBool::new(false),
            config_version: AtomicU64::new(1),
        }
    }

    pub fn uptime_ms(&self) -> u64 {
        self.session_start.elapsed().as_millis() as u64
    }

    pub fn to_json(&self) -> Value {
        json!({
            "frames": self.frames_processed.load(Ordering::Relaxed),
            "decisions": self.decisions_made.load(Ordering::Relaxed),
            "kills": self.kills.load(Ordering::Relaxed),
            "deaths": self.deaths.load(Ordering::Relaxed),
            "potions": self.potions_used.load(Ordering::Relaxed),
            "loots": self.loots_picked.load(Ordering::Relaxed),
            "chickens": self.chickens.load(Ordering::Relaxed),
            "uptime_ms": self.uptime_ms(),
            "paused": self.paused.load(Ordering::Relaxed),
            "config_version": self.config_version.load(Ordering::Relaxed),
        })
    }
}

impl Default for SharedAgentStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Commands the Chrome extension can send to the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum ChromeCommand {
    #[serde(rename = "handshake")]
    Handshake {
        version: Option<String>,
        #[serde(rename = "extensionId")]
        extension_id: Option<String>,
    },
    #[serde(rename = "ping")]
    Ping { timestamp: Option<i64> },
    #[serde(rename = "get_stats")]
    GetStats,
    #[serde(rename = "get_buffer_stats")]
    GetBufferStats,
    #[serde(rename = "pause")]
    Pause { reason: Option<String> },
    #[serde(rename = "resume")]
    Resume,
    #[serde(rename = "shutdown")]
    Shutdown,
    #[serde(rename = "update_config")]
    UpdateConfig { data: Option<Value> },
}

/// The native messaging host. Owns the stdio pipe and bridges
/// Chrome <-> Agent communication.
pub struct NativeMessagingHost {
    agent_stats: Arc<SharedAgentStats>,
    frame_buffer: Arc<ShardedFrameBuffer>,
    /// Channel to send commands INTO the agent
    agent_cmd_tx: mpsc::UnboundedSender<AgentCommand>,
    /// Version string for handshake
    version: String,
}

/// Commands sent from the host to the agent's main loop
#[derive(Debug, Clone)]
pub enum AgentCommand {
    Pause(String),
    Resume,
    Shutdown,
    UpdateConfig(Value),
}

impl NativeMessagingHost {
    pub fn new(
        agent_stats: Arc<SharedAgentStats>,
        frame_buffer: Arc<ShardedFrameBuffer>,
    ) -> (Self, mpsc::UnboundedReceiver<AgentCommand>) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let host = Self {
            agent_stats,
            frame_buffer,
            agent_cmd_tx: cmd_tx,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        (host, cmd_rx)
    }

    /// Run the native messaging event loop.
    /// This blocks until the Chrome pipe closes (Chrome exit/extension unload).
    pub async fn run(self) {
        let mut stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();

        loop {
            match Self::read_message(&mut stdin).await {
                Ok(Some(msg)) => {
                    if let Some(response) = self.handle_message(msg) {
                        if Self::send_message(&mut stdout, &response).await.is_err() {
                            break; // Pipe broken
                        }
                    }
                }
                Ok(None) => break, // EOF — Chrome closed the pipe
                Err(_) => break,   // Read error — pipe broken
            }
        }

        // Graceful shutdown: give agent 2 seconds to clean up
        let _ = self.agent_cmd_tx.send(AgentCommand::Shutdown);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    /// Read one native messaging frame: 4-byte LE length + JSON
    async fn read_message(stdin: &mut tokio::io::Stdin) -> anyhow::Result<Option<Value>> {
        let mut len_bytes = [0u8; 4];
        match stdin.read_exact(&mut len_bytes).await {
            Ok(_) => {
                let len = u32::from_le_bytes(len_bytes) as usize;
                if len > 1024 * 1024 {
                    // 1MB safety limit — reject absurd messages
                    anyhow::bail!("message too large: {} bytes", len);
                }
                let mut buf = vec![0u8; len];
                stdin.read_exact(&mut buf).await?;
                Ok(Some(serde_json::from_slice(&buf)?))
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Write one native messaging frame: 4-byte LE length + JSON
    async fn send_message(stdout: &mut tokio::io::Stdout, msg: &Value) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec(msg)?;
        let len = bytes.len() as u32;
        stdout.write_all(&len.to_le_bytes()).await?;
        stdout.write_all(&bytes).await?;
        stdout.flush().await?;
        Ok(())
    }

    /// Handle an incoming Chrome message. Returns optional response.
    pub fn handle_message(&self, msg: Value) -> Option<Value> {
        let cmd = msg.get("cmd").and_then(|c| c.as_str()).unwrap_or("");

        match cmd {
            "handshake" => Some(json!({
                "cmd": "handshake_ack",
                "version": self.version,
                "pid": std::process::id(),
                "timestamp": chrono::Utc::now().timestamp_millis(),
            })),

            "ping" => Some(json!({
                "cmd": "pong",
                "timestamp": msg.get("timestamp"),
                "server_time": chrono::Utc::now().timestamp_millis(),
            })),

            "get_stats" => Some(json!({
                "cmd": "stats",
                "data": self.agent_stats.to_json(),
            })),

            "get_buffer_stats" => {
                let bs = self.frame_buffer.stats();
                Some(json!({
                    "cmd": "buffer_stats",
                    "data": {
                        "total_frames": bs.total_frames_written,
                        "shards_complete": bs.shards_complete,
                        "shards_writing": bs.shards_writing,
                        "shards_idle": bs.shards_idle,
                        "current_shard": bs.current_shard,
                    }
                }))
            }

            "pause" => {
                self.agent_stats.paused.store(true, Ordering::Relaxed);
                let reason = msg
                    .get("reason")
                    .and_then(|r| r.as_str())
                    .unwrap_or("manual")
                    .to_string();
                let _ = self.agent_cmd_tx.send(AgentCommand::Pause(reason));
                Some(json!({ "cmd": "ack", "action": "paused" }))
            }

            "resume" => {
                self.agent_stats.paused.store(false, Ordering::Relaxed);
                let _ = self.agent_cmd_tx.send(AgentCommand::Resume);
                Some(json!({ "cmd": "ack", "action": "resumed" }))
            }

            "shutdown" => {
                let _ = self.agent_cmd_tx.send(AgentCommand::Shutdown);
                Some(json!({ "cmd": "ack", "action": "shutdown" }))
            }

            "update_config" => {
                if let Some(data) = msg.get("data").cloned() {
                    self.agent_stats
                        .config_version
                        .fetch_add(1, Ordering::Relaxed);
                    let _ = self.agent_cmd_tx.send(AgentCommand::UpdateConfig(data));
                    Some(json!({ "cmd": "ack", "action": "config_updated" }))
                } else {
                    Some(json!({ "cmd": "error", "message": "missing data field" }))
                }
            }

            // Returns the most recent FrameState for the debug overlay relay.
            // The map host cannot read this directly (different process), so the
            // extension relays it via update_debug_state every 100 ms when the
            // debug overlay is active.
            "get_frame_state" => {
                match self.frame_buffer.latest() {
                    Some(s) => Some(json!({
                        "cmd": "frame_state",
                        "hp_pct":               s.hp_pct,
                        "mana_pct":             s.mana_pct,
                        "merc_hp_pct":          s.merc_hp_pct,
                        "merc_alive":           s.merc_alive,
                        "enemy_count":          s.enemy_count,
                        "in_combat":            s.in_combat,
                        "nearest_enemy_x":      s.nearest_enemy_x,
                        "nearest_enemy_y":      s.nearest_enemy_y,
                        "nearest_enemy_hp_pct": s.nearest_enemy_hp_pct,
                        "in_town":              s.in_town,
                        "at_menu":              s.at_menu,
                        "loading_screen":       s.loading_screen,
                        "area_name":            s.area_name_str(),
                        "char_screen_x":        s.char_screen_x,
                        "char_screen_y":        s.char_screen_y,
                        "frame_width":          s.frame_width,
                        "frame_height":         s.frame_height,
                        "tick":                 s.tick,
                    })),
                    None => Some(json!({
                        "cmd": "frame_state",
                        "available": false,
                    })),
                }
            }

            _ => Some(json!({
                "cmd": "error",
                "message": "unknown_command",
                "received": cmd,
            })),
        }
    }
}

/// Simulated pipe for testing — mimics Chrome's stdio protocol
/// using in-memory byte streams.
/// Pipe encoding/decoding utilities for the native messaging protocol.
/// Used for testing and by external tools that simulate Chrome's stdio pipe.
pub mod test_pipe {
    use serde_json::Value;

    /// Encode a message in native messaging format (4-byte LE length + JSON)
    pub fn encode_message(msg: &Value) -> Vec<u8> {
        let json_bytes = serde_json::to_vec(msg).unwrap();
        let len = json_bytes.len() as u32;
        let mut buf = Vec::with_capacity(4 + json_bytes.len());
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&json_bytes);
        buf
    }

    /// Decode a message from native messaging format
    pub fn decode_message(data: &[u8]) -> Option<(Value, usize)> {
        if data.len() < 4 {
            return None;
        }
        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return None;
        }
        let msg: Value = serde_json::from_slice(&data[4..4 + len]).ok()?;
        Some((msg, 4 + len))
    }

    /// Decode all messages from a byte buffer
    pub fn decode_all_messages(data: &[u8]) -> Vec<Value> {
        let mut messages = Vec::new();
        let mut offset = 0;
        while offset < data.len() {
            if let Some((msg, consumed)) = decode_message(&data[offset..]) {
                messages.push(msg);
                offset += consumed;
            } else {
                break;
            }
        }
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::ShardedFrameBuffer;

    fn make_host() -> (NativeMessagingHost, mpsc::UnboundedReceiver<AgentCommand>) {
        let stats = Arc::new(SharedAgentStats::new());
        let buffer = Arc::new(ShardedFrameBuffer::new());
        NativeMessagingHost::new(stats, buffer)
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        use test_pipe::*;

        let msg = json!({"cmd": "ping", "timestamp": 12345});
        let encoded = encode_message(&msg);

        // Check length prefix
        let len = u32::from_le_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]);
        assert!(len > 0 && len < 1000);

        // Decode
        let (decoded, consumed) = decode_message(&encoded).unwrap();
        assert_eq!(decoded["cmd"], "ping");
        assert_eq!(decoded["timestamp"], 12345);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_decode_multiple_messages() {
        use test_pipe::*;

        let mut buf = Vec::new();
        buf.extend_from_slice(&encode_message(&json!({"cmd": "ping", "n": 1})));
        buf.extend_from_slice(&encode_message(&json!({"cmd": "ping", "n": 2})));
        buf.extend_from_slice(&encode_message(&json!({"cmd": "get_stats"})));

        let messages = decode_all_messages(&buf);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["n"], 1);
        assert_eq!(messages[1]["n"], 2);
        assert_eq!(messages[2]["cmd"], "get_stats");
    }

    #[test]
    fn test_handshake_response() {
        let (host, _rx) = make_host();

        let msg = json!({"cmd": "handshake", "version": "2.1.0", "extensionId": "abc123"});
        let resp = host.handle_message(msg).unwrap();

        assert_eq!(resp["cmd"], "handshake_ack");
        assert!(resp.get("version").is_some());
        assert!(resp.get("pid").is_some());
        assert!(resp.get("timestamp").is_some());
    }

    #[test]
    fn test_ping_pong() {
        let (host, _rx) = make_host();

        let msg = json!({"cmd": "ping", "timestamp": 1708000000000i64});
        let resp = host.handle_message(msg).unwrap();

        assert_eq!(resp["cmd"], "pong");
        assert_eq!(resp["timestamp"], 1708000000000i64);
        assert!(resp.get("server_time").is_some());
    }

    #[test]
    fn test_get_stats() {
        let (host, _rx) = make_host();

        // Simulate some activity
        host.agent_stats
            .frames_processed
            .store(42000, Ordering::Relaxed);
        host.agent_stats.kills.store(150, Ordering::Relaxed);
        host.agent_stats.deaths.store(2, Ordering::Relaxed);
        host.agent_stats.potions_used.store(340, Ordering::Relaxed);

        let resp = host.handle_message(json!({"cmd": "get_stats"})).unwrap();

        assert_eq!(resp["cmd"], "stats");
        assert_eq!(resp["data"]["frames"], 42000);
        assert_eq!(resp["data"]["kills"], 150);
        assert_eq!(resp["data"]["deaths"], 2);
        assert_eq!(resp["data"]["potions"], 340);
        assert!(resp["data"]["uptime_ms"].as_u64().is_some());
    }

    #[test]
    fn test_get_buffer_stats() {
        let (host, _rx) = make_host();

        // Push some frames
        for i in 0..50u64 {
            let mut state = crate::vision::FrameState::default();
            state.tick = i;
            host.frame_buffer.push(state);
        }

        let resp = host
            .handle_message(json!({"cmd": "get_buffer_stats"}))
            .unwrap();

        assert_eq!(resp["cmd"], "buffer_stats");
        assert_eq!(resp["data"]["total_frames"], 50);
        assert!(resp["data"]["shards_complete"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_pause_resume() {
        let (host, mut rx) = make_host();

        // Pause
        assert!(!host.agent_stats.paused.load(Ordering::Relaxed));
        let resp = host
            .handle_message(json!({"cmd": "pause", "reason": "manual"}))
            .unwrap();
        assert_eq!(resp["action"], "paused");
        assert!(host.agent_stats.paused.load(Ordering::Relaxed));

        // Check command received
        let cmd = rx.try_recv().unwrap();
        assert!(matches!(cmd, AgentCommand::Pause(r) if r == "manual"));

        // Resume
        let resp = host.handle_message(json!({"cmd": "resume"})).unwrap();
        assert_eq!(resp["action"], "resumed");
        assert!(!host.agent_stats.paused.load(Ordering::Relaxed));

        let cmd = rx.try_recv().unwrap();
        assert!(matches!(cmd, AgentCommand::Resume));
    }

    #[test]
    fn test_shutdown_command() {
        let (host, mut rx) = make_host();

        let resp = host.handle_message(json!({"cmd": "shutdown"})).unwrap();
        assert_eq!(resp["action"], "shutdown");

        let cmd = rx.try_recv().unwrap();
        assert!(matches!(cmd, AgentCommand::Shutdown));
    }

    #[test]
    fn test_config_update() {
        let (host, mut rx) = make_host();

        let initial_version = host.agent_stats.config_version.load(Ordering::Relaxed);

        let resp = host
            .handle_message(json!({
                "cmd": "update_config",
                "data": {"survival": {"hp_potion_pct": 80}}
            }))
            .unwrap();
        assert_eq!(resp["action"], "config_updated");

        let new_version = host.agent_stats.config_version.load(Ordering::Relaxed);
        assert_eq!(new_version, initial_version + 1);

        let cmd = rx.try_recv().unwrap();
        match cmd {
            AgentCommand::UpdateConfig(data) => {
                assert_eq!(data["survival"]["hp_potion_pct"], 80);
            }
            _ => panic!("expected UpdateConfig"),
        }
    }

    #[test]
    fn test_config_update_missing_data() {
        let (host, _rx) = make_host();

        let resp = host
            .handle_message(json!({"cmd": "update_config"}))
            .unwrap();
        assert_eq!(resp["cmd"], "error");
        assert!(resp["message"].as_str().unwrap().contains("missing"));
    }

    #[test]
    fn test_unknown_command() {
        let (host, _rx) = make_host();

        let resp = host
            .handle_message(json!({"cmd": "do_something_weird"}))
            .unwrap();
        assert_eq!(resp["cmd"], "error");
        assert_eq!(resp["received"], "do_something_weird");
    }

    #[test]
    fn test_shared_stats_concurrent() {
        let stats = Arc::new(SharedAgentStats::new());
        let stats2 = Arc::clone(&stats);

        // Simulate agent thread incrementing stats
        let handle = std::thread::spawn(move || {
            for _ in 0..10000 {
                stats2.frames_processed.fetch_add(1, Ordering::Relaxed);
                stats2.decisions_made.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Simultaneously read stats (simulates host reading for Chrome)
        let mut last_frames = 0u64;
        let mut monotonic = true;
        for _ in 0..1000 {
            let f = stats.frames_processed.load(Ordering::Relaxed);
            if f < last_frames {
                monotonic = false;
            }
            last_frames = f;
        }

        handle.join().unwrap();

        assert!(monotonic, "frame counter should be monotonic");
        assert_eq!(stats.frames_processed.load(Ordering::Relaxed), 10000);
    }

    #[test]
    fn test_stats_json_snapshot() {
        let stats = SharedAgentStats::new();
        stats.frames_processed.store(100, Ordering::Relaxed);
        stats.kills.store(5, Ordering::Relaxed);
        stats.paused.store(true, Ordering::Relaxed);

        let json = stats.to_json();
        assert_eq!(json["frames"], 100);
        assert_eq!(json["kills"], 5);
        assert_eq!(json["paused"], true);
        assert!(json["uptime_ms"].as_u64().unwrap() >= 0);
    }
}
