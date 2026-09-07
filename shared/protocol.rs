use serde::{Deserialize, Serialize};

/// The UI payload has no diagnostics, paths or wall-clock formatting.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    pub status: String,
    pub hint: String,
    pub automatic_status: String,
    pub automatic: bool,
    pub autostart: bool,
    pub tray_only: bool,
    pub busy: bool,
    pub can_repair: bool,
    pub awaiting_confirmation: bool,
    pub revision: u64,
    pub wait_remaining_ms: u64,
}

pub fn change_event(session: u32) -> String {
    format!("Local\\CodexPetRepair-ui-state-v1-{session}")
}
