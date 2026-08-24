use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TunnelState {
    Stopped,
    Connecting,
    Connected { active_connections: u32 },
    Error(String),
    Stopping,
}
