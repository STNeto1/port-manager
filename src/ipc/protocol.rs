use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    pub request_id: u64,
    pub request: ClientRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientRequest {
    Ping,
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
}
