use std::sync::Arc;

use color_eyre::eyre::Result;
use russh::client;
use russh::keys::PublicKeyOrCertificate;
use tracing::warn;
use uuid::Uuid;

use crate::config::schema::TunnelDefinition;

use super::auth;

pub struct Client {
    tunnel_id: Uuid,
}

impl Client {
    pub fn new(tunnel_id: Uuid) -> Self {
        Self { tunnel_id }
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    /// MVP: accept any host key. There is no `known_hosts` verification yet
    /// — a real man-in-the-middle exposure, logged loudly on every
    /// connection rather than shipped as a silent default.
    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        warn!(
            tunnel_id = %self.tunnel_id,
            "accepting SSH host key without verification (no known_hosts checking yet)"
        );
        Ok(true)
    }
}

/// An authenticated session to a tunnel's target host, transparently hopping
/// through `def.jump` first if set. When jumping, the jump host's own
/// session must be kept alive for as long as the target session is used —
/// the channel it opened is what the target session's stream runs over —
/// so both handles are held here rather than only returning the target one.
pub struct Connection {
    pub target: client::Handle<Client>,
    _jump_guard: Option<client::Handle<Client>>,
}

pub async fn connect(tunnel_id: Uuid, def: &TunnelDefinition) -> Result<Connection> {
    let config = Arc::new(client::Config {
        nodelay: true,
        ..Default::default()
    });

    let connection = match &def.jump {
        None => {
            let mut handle = client::connect(
                config,
                (def.host.as_str(), def.port),
                Client::new(tunnel_id),
            )
            .await?;
            auth::authenticate(&mut handle, &def.username, &def.auth).await?;
            Connection {
                target: handle,
                _jump_guard: None,
            }
        }
        Some(jump) => {
            let (jump_handle, mut target_handle) =
                super::jump::connect_through(tunnel_id, jump, &def.host, def.port, config).await?;
            auth::authenticate(&mut target_handle, &def.username, &def.auth).await?;
            Connection {
                target: target_handle,
                _jump_guard: Some(jump_handle),
            }
        }
    };

    Ok(connection)
}
