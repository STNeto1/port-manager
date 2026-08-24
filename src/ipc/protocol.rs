use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::schema::TunnelDefinition;
use crate::model::{TunnelEvent, TunnelState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    pub request_id: u64,
    pub request: ClientRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientRequest {
    Ping,
    ListTunnels,
    AddTunnel(TunnelDefinition),
    UpdateTunnel(TunnelDefinition),
    RemoveTunnel(Uuid),
    StartTunnel(Uuid),
    StopTunnel(Uuid),
    ReloadConfig,
    /// Puts this connection into event-streaming mode: after the Ack
    /// response, the daemon only ever pushes `DaemonMessage::Event`s on it.
    Subscribe,
    ShutdownDaemon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonMessage {
    Response {
        request_id: u64,
        result: Result<ResponsePayload, String>,
    },
    Event(TunnelEvent),
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
