use std::sync::Arc;

use color_eyre::eyre::Result;
use russh::Channel;
use russh::client::{self, ChannelOpenHandle, Msg, Session};
use russh::keys::PublicKeyOrCertificate;
use tracing::warn;
use uuid::Uuid;

use crate::config::schema::{Direction, HostPort, TunnelDefinition};

use super::auth;

pub struct Client {
    tunnel_id: Uuid,
    /// Set only for Remote-direction tunnels: where to proxy each inbound
    /// `forwarded-tcpip` channel the server opens on us.
    remote_forward_target: Option<HostPort>,
}

impl Client {
    pub fn new(tunnel_id: Uuid) -> Self {
        Self {
            tunnel_id,
            remote_forward_target: None,
        }
    }

    pub fn with_remote_forward_target(tunnel_id: Uuid, target: HostPort) -> Self {
        Self {
            tunnel_id,
            remote_forward_target: Some(target),
        }
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

    /// Only ever fires for a Remote-direction tunnel's target session,
    /// since only those call `tcpip_forward`. Accepts every inbound
    /// forwarded connection and hands it off to a background task that
    /// proxies it to `remote_forward_target`.
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        _connected_address: &str,
        _connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;

        let tunnel_id = self.tunnel_id;
        if let Some(target) = self.remote_forward_target.clone() {
            tokio::spawn(async move {
                if let Err(err) =
                    super::remote_forward::proxy_forwarded_channel(channel, target).await
                {
                    warn!(tunnel_id = %tunnel_id, ?err, "remote-forward connection proxy failed");
                }
            });
        }

        Ok(())
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

    let target_client = match def.direction {
        Direction::Remote => {
            let target = def.remote.clone().expect(
                "Remote direction always has a local forward target (form/config validated this)",
            );
            Client::with_remote_forward_target(tunnel_id, target)
        }
        Direction::Local | Direction::Dynamic => Client::new(tunnel_id),
    };

    let connection = match &def.jump {
        None => {
            let mut handle =
                client::connect(config, (def.host.as_str(), def.port), target_client).await?;
            auth::authenticate(&mut handle, &def.username, &def.auth).await?;
            Connection {
                target: handle,
                _jump_guard: None,
            }
        }
        Some(jump) => {
            let (jump_handle, mut target_handle) = super::jump::connect_through(
                tunnel_id,
                jump,
                &def.host,
                def.port,
                config,
                target_client,
            )
            .await?;
            auth::authenticate(&mut target_handle, &def.username, &def.auth).await?;
            Connection {
                target: target_handle,
                _jump_guard: Some(jump_handle),
            }
        }
    };

    Ok(connection)
}
