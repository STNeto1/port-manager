use std::net::SocketAddr;
use std::sync::Arc;

use color_eyre::eyre::Result;
use russh::client::Handle;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::config::schema::{HostPort, SocketAddrSpec};

use super::client::Client;

/// Accepts local TCP connections and proxies each one, over its own
/// direct-tcpip channel on the shared session, to `remote`. Runs until a
/// `Stop` is received on `stop_rx` (including the sender being dropped,
/// which is how the daemon tears a tunnel down when its definition is
/// removed or replaced out from under it).
pub async fn run(
    handle: Arc<Handle<Client>>,
    local_bind: &SocketAddrSpec,
    remote: &HostPort,
    mut stop_rx: mpsc::Receiver<()>,
) -> Result<()> {
    let listener = TcpListener::bind((local_bind.bind_addr.as_str(), local_bind.port)).await?;
    info!(addr = %local_bind.bind_addr, port = local_bind.port, "local forward listening");

    let mut proxies = JoinSet::new();

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                let handle = Arc::clone(&handle);
                let remote_host = remote.host.clone();
                let remote_port = remote.port as u32;
                proxies.spawn(async move {
                    if let Err(err) = proxy_connection(&handle, stream, peer, remote_host, remote_port).await {
                        warn!(?err, "local-forward connection proxy failed");
                    }
                });
            }
            _ = stop_rx.recv() => {
                info!("stopping local forward listener");
                break;
            }
        }
    }

    proxies.shutdown().await;
    Ok(())
}

async fn proxy_connection(
    handle: &Handle<Client>,
    mut local: TcpStream,
    peer: SocketAddr,
    remote_host: String,
    remote_port: u32,
) -> Result<()> {
    let channel = handle
        .channel_open_direct_tcpip(
            remote_host,
            remote_port,
            peer.ip().to_string(),
            peer.port() as u32,
        )
        .await?;
    let mut remote_stream = channel.into_stream();
    copy_bidirectional(&mut local, &mut remote_stream).await?;
    Ok(())
}
