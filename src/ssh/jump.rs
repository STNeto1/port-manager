use std::sync::Arc;

use color_eyre::eyre::Result;
use russh::client;
use uuid::Uuid;

use crate::config::resolve::ResolvedJump;

use super::auth;
use super::client::Client;

/// Connects and authenticates through each hop in `jumps` in order (entry
/// point first), opening a direct-tcpip channel from each hop to the next
/// and layering a new SSH session on top of that channel's stream via
/// `client::connect_stream` — the standard way to implement ProxyJump
/// without shelling out to `ssh`, extended to an arbitrary-length chain.
/// Finally opens a channel from the last hop to `target_host:target_port`
/// for the target session. Returns every intermediate handle (the caller
/// must keep them alive for as long as the target handle is in use) plus
/// the target handle itself.
pub async fn connect_through(
    tunnel_id: Uuid,
    jumps: &[ResolvedJump],
    target_host: &str,
    target_port: u16,
    config: Arc<client::Config>,
    target_client: Client,
) -> Result<(Vec<client::Handle<Client>>, client::Handle<Client>)> {
    let mut handles: Vec<client::Handle<Client>> = Vec::with_capacity(jumps.len());

    let first = &jumps[0];
    let mut handle = client::connect(
        Arc::clone(&config),
        (first.host.as_str(), first.port),
        Client::new(tunnel_id, first.host.as_str(), first.port),
    )
    .await?;
    auth::authenticate(&mut handle, &first.username, &first.auth).await?;
    handles.push(handle);

    for hop in &jumps[1..] {
        let channel = handles
            .last()
            .expect("handles is non-empty: the first hop was just pushed")
            .channel_open_direct_tcpip(&hop.host, hop.port as u32, "127.0.0.1", 0)
            .await?;
        let mut next_handle = client::connect_stream(
            Arc::clone(&config),
            channel.into_stream(),
            Client::new(tunnel_id, hop.host.as_str(), hop.port),
        )
        .await?;
        auth::authenticate(&mut next_handle, &hop.username, &hop.auth).await?;
        handles.push(next_handle);
    }

    let channel = handles
        .last()
        .expect("handles is non-empty: at least the first hop is always pushed")
        .channel_open_direct_tcpip(target_host, target_port as u32, "127.0.0.1", 0)
        .await?;
    let target_handle =
        client::connect_stream(config, channel.into_stream(), target_client).await?;

    Ok((handles, target_handle))
}
