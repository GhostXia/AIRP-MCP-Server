//! Gating and Checkpoints model

use serde::{Deserialize, Serialize};

/// Gating configuration for progressive content unlocking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatingConfig {
    /// List of checkpoints
    pub checkpoints: Vec<Checkpoint>,
    /// Current checkpoint index
    pub current: Option<String>,
    /// Total turns count
    pub turn_count: u64,
}

/// A single checkpoint definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique checkpoint ID
    pub id: String,
    /// Label for display
    pub label: String,
    /// Number of turns required to reach this checkpoint
    pub turns_required: u64,
    /// Whether this checkpoint has been reached
    pub reached: bool,
    /// Additional slots/data associated with this checkpoint
    pub slots: Vec<CheckpointSlot>,
}

/// Data slot within a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSlot {
    pub key: String,
    pub value: serde_json::Value,
}

/// Timeline entry for tracking progression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub turn: u64,
    pub checkpoint_id: Option<String>,
    pub description: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl GatingConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance turn counter and check for checkpoint triggers
    pub fn advance(&mut self) -> Option<&Checkpoint> {
        self.turn_count += 1;
        self.check_current()
    }

    /// Check if current turn triggers any checkpoint
    pub fn check_current(&self) -> Option<&Checkpoint> {
        self.checkpoints
            .iter()
            .rev()
            .find(|cp| !cp.reached && self.turn_count >= cp.turns_required)
    }

    /// Mark a checkpoint as reached
    pub fn reach_checkpoint(&mut self, checkpoint_id: &str) -> bool {
        if let Some(cp) = self.checkpoints.iter_mut().find(|c| c.id == checkpoint_id) {
            cp.reached = true;
            self.current = Some(checkpoint_id.to_string());
            true
        } else {
            false
        }
    }

    /// Get checkpoint status summary
    pub fn status(&self) -> String {
        let total = self.checkpoints.len();
        let reached = self.checkpoints.iter().filter(|c| c.reached).count();
        format!(
            "Turn {}/{}, Checkpoints: {}/{} reached",
            self.turn_count,
            self.checkpoints
                .last()
                .map(|c| c.turns_required)
                .unwrap_or(0),
            reached,
            total,
        )
    }

    /// Get next unreached checkpoint
    pub fn next_unreached(&self) -> Option<&Checkpoint> {
        self.checkpoints.iter().find(|c| !c.reached)
    }
}
