use crate::decision::Decision;
use crate::vision::FrameState;
use chrono::Utc;
use serde::Serialize;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[derive(Serialize)]
struct LogEntry {
    timestamp: String,
    tick: u64,
    tick_phase_ms: u16,
    phase_confidence: f32,
    state: StateSnapshot,
    action_type: String,
    action_detail: serde_json::Value,
    delay_ms: u64,
    priority: u8,
    reason: String,
}

#[derive(Serialize)]
struct StateSnapshot {
    hp_pct: u8,
    mana_pct: u8,
    enemy_count: u8,
    in_combat: bool,
    in_town: bool,
    loot_labels: u8,
    merc_alive: bool,
    motion_magnitude: f32,
}

pub struct TrainingLogger {
    tx: mpsc::Sender<LogEntry>,
}

impl TrainingLogger {
    /// Spawn async writer task that drains log entries to JSONL file.
    pub fn new(log_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel(1024);

        tokio::spawn(Self::writer_task(rx, log_dir));

        Self { tx }
    }

    async fn writer_task(mut rx: mpsc::Receiver<LogEntry>, log_dir: PathBuf) {
        std::fs::create_dir_all(&log_dir).ok();
        let filename = format!("session_{}.jsonl", Utc::now().format("%Y%m%d_%H%M%S"));
        let path = log_dir.join(filename);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .expect("failed to open log file");

        let mut buf = Vec::with_capacity(4096);
        let mut count = 0u64;

        while let Some(entry) = rx.recv().await {
            buf.clear();
            serde_json::to_writer(&mut buf, &entry).ok();
            buf.push(b'\n');
            file.write_all(&buf).await.ok();

            count += 1;
            // Flush every 100 entries
            if count % 100 == 0 {
                file.flush().await.ok();
            }
        }

        file.flush().await.ok();
    }

    /// Log a decision (non-blocking, drops if channel full).
    pub fn log(&self, state: &FrameState, decision: &Decision) {
        let (action_type, action_detail) = format_action(&decision.action);

        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            tick: state.tick,
            tick_phase_ms: state.tick_phase_ms,
            phase_confidence: state.phase_confidence,
            state: StateSnapshot {
                hp_pct: state.hp_pct,
                mana_pct: state.mana_pct,
                enemy_count: state.enemy_count,
                in_combat: state.in_combat,
                in_town: state.in_town,
                loot_labels: state.loot_label_count,
                merc_alive: state.merc_alive,
                motion_magnitude: state.motion_magnitude,
            },
            action_type,
            action_detail,
            delay_ms: decision.delay.as_millis() as u64,
            priority: decision.priority,
            reason: decision.reason.to_string(),
        };

        // Non-blocking send — drop if channel full (acceptable for training data)
        let _ = self.tx.try_send(entry);
    }
}

fn format_action(action: &crate::decision::Action) -> (String, serde_json::Value) {
    use crate::decision::Action;
    match action {
        Action::DrinkPotion { belt_slot } => (
            "drink_potion".into(),
            serde_json::json!({ "belt_slot": belt_slot }),
        ),
        Action::CastSkill {
            key,
            screen_x,
            screen_y,
        } => (
            "cast_skill".into(),
            serde_json::json!({ "key": key.to_string(), "x": screen_x, "y": screen_y }),
        ),
        Action::PickupLoot { screen_x, screen_y } => (
            "pickup_loot".into(),
            serde_json::json!({ "x": screen_x, "y": screen_y }),
        ),
        Action::MoveTo { screen_x, screen_y } => (
            "move_to".into(),
            serde_json::json!({ "x": screen_x, "y": screen_y }),
        ),
        Action::TownPortal => ("town_portal".into(), serde_json::json!({})),
        Action::ChickenQuit => ("chicken_quit".into(), serde_json::json!({})),
        Action::RecastBuff { key } => (
            "recast_buff".into(),
            serde_json::json!({ "key": key.to_string() }),
        ),
        Action::TakeBreak { duration } => (
            "take_break".into(),
            serde_json::json!({ "duration_secs": duration.as_secs() }),
        ),
        Action::IdlePause { duration } => (
            "idle_pause".into(),
            serde_json::json!({ "duration_ms": duration.as_millis() as u64 }),
        ),
        Action::Dodge { screen_x, screen_y } => (
            "dodge".into(),
            serde_json::json!({ "x": screen_x, "y": screen_y }),
        ),
        Action::SwitchWeapon => ("switch_weapon".into(), serde_json::json!({})),
        Action::Wait => ("wait".into(), serde_json::json!({})),
    }
}
