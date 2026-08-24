use std::sync::Arc;

use color_eyre::eyre::Result;
use russh::Channel;
use russh::client::{Handle, Msg};
use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::info;

use crate::config::schema::{HostPort, SocketAddrSpec};

use super::client::Client;

/// Requests the server forward its `bind` address/port back to us, then
/// idles until stopped. Unlike local forwarding there is no accept loop to
/// run here: inbound connections arrive as `forwarded-tcpip` channel-open
/// requests, which the `Client` Handler's `server_channel_open_forwarded_tcpip`
/// callback (in `client.rs`) handles as they come in via `proxy_forwarded_channel` below.
pub async fn run(
    handle: Arc<Handle<Client>>,
    bind: &SocketAddrSpec,
    mut stop_rx: mpsc::Receiver<()>,
) -> Result<()> {
    handle
        .tcpip_forward(bind.bind_addr.clone(), bind.port as u32)
        .await?;
    info!(addr = %bind.bind_addr, port = bind.port, "remote forward requested");

    let _ = stop_rx.recv().await;

    let _ = handle
        .cancel_tcpip_forward(bind.bind_addr.clone(), bind.port as u32)
        .await;
    Ok(())
}

pub(crate) async fn proxy_forwarded_channel(channel: Channel<Msg>, target: HostPort) -> Result<()> {
    let mut local = TcpStream::connect((target.host.as_str(), target.port)).await?;
    let mut remote_stream = channel.into_stream();
    copy_bidirectional(&mut local, &mut remote_stream).await?;
    Ok(())
}
