use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TunnelState {
    Stopped,
    Connecting,
    Connected { active_connections: u32 },
    Error(String),
    Stopping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TunnelEvent {
    StateChanged(Uuid, TunnelState),
}
