use serde::{Deserialize, Serialize};

use crate::config::schema::TunnelDefinition;
use crate::model::TunnelState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    pub request_id: u64,
    pub request: ClientRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientRequest {
    Ping,
    ListTunnels,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonMessage {
    Response {
        request_id: u64,
        result: Result<ResponsePayload, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponsePayload {
    Ack,
    Tunnels(Vec<TunnelSnapshot>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelSnapshot {
    pub def: TunnelDefinition,
    pub state: TunnelState,
}
