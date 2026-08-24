use std::sync::Arc;

use color_eyre::eyre::Result;
use russh::client;
use uuid::Uuid;

use crate::config::schema::JumpHost;

use super::auth;
use super::client::Client;

/// Connects and authenticates to `jump`, then opens a direct-tcpip channel
/// from it to `target_host:target_port` and layers a *second* SSH session
/// on top of that channel's stream via `client::connect_stream` — the
/// standard way to implement ProxyJump without shelling out to `ssh`.
/// Returns both handles: the caller must keep the jump handle alive for as
/// long as the target handle is in use.
pub async fn connect_through(
    tunnel_id: Uuid,
    jump: &JumpHost,
    target_host: &str,
    target_port: u16,
    config: Arc<client::Config>,
    target_client: Client,
) -> Result<(client::Handle<Client>, client::Handle<Client>)> {
    let mut jump_handle = client::connect(
        Arc::clone(&config),
        (jump.host.as_str(), jump.port),
        Client::new(tunnel_id, jump.host.as_str(), jump.port),
    )
    .await?;
    auth::authenticate(&mut jump_handle, &jump.username, &jump.auth).await?;

    let channel = jump_handle
        .channel_open_direct_tcpip(target_host, target_port as u32, "127.0.0.1", 0)
        .await?;
    let stream = channel.into_stream();

    let target_handle = client::connect_stream(config, stream, target_client).await?;

    Ok((jump_handle, target_handle))
}
