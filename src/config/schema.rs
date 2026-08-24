use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub tunnels: Vec<TunnelDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelDefinition {
    pub id: Uuid,
    pub name: String,
    pub direction: Direction,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    #[serde(default)]
    pub jump: Option<JumpHost>,
    pub local_bind: SocketAddrSpec,
    #[serde(default)]
    pub remote: Option<HostPort>,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Local,
    Remote,
    Dynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMethod {
    Password { password: Option<String> },
    PrivateKey { path: PathBuf, passphrase: Option<String> },
    Agent,
}

/// A single-hop ProxyJump host: the daemon connects and authenticates here
/// first, then opens the target SSH session over a direct-tcpip channel from
/// this host. Modeled on ~/.config/ssh/config's `worker`/`aldea` hosts, which
/// are only reachable via `ProxyJump nixserver`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JumpHost {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostPort {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketAddrSpec {
    pub bind_addr: String,
    pub port: u16,
}
