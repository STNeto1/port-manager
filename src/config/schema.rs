use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub tunnels: Vec<TunnelDefinition>,
}

/// A reusable SSH connection — host/port/username/auth, and optionally a
/// jump host (itself another profile's `name`, single-hop only). Any number
/// of tunnels can reference the same profile by name instead of repeating
/// its connection details, the same way an ssh_config `Host` block covers
/// every `LocalForward` line under it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    /// Name of another profile to jump through. That profile must not
    /// itself have a `jump` set — chained (multi-hop) jumps aren't
    /// supported yet.
    #[serde(default)]
    pub jump: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelDefinition {
    pub id: Uuid,
    pub name: String,
    pub direction: Direction,
    /// References a `Profile.name`. Resolved at start time, not validated
    /// when the tunnel is added/edited.
    pub profile: String,
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
    Password {
        password: Option<String>,
    },
    PrivateKey {
        path: PathBuf,
        passphrase: Option<String>,
    },
    Agent,
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
