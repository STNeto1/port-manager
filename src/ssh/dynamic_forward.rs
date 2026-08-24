use std::net::SocketAddr;
use std::sync::Arc;

use color_eyre::eyre::Result;
use fast_socks5::server::Socks5ServerProtocol;
use fast_socks5::util::target_addr::TargetAddr;
use fast_socks5::{ReplyError, Socks5Command};
use russh::client::Handle;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::config::schema::SocketAddrSpec;

use super::client::Client;

/// Accepts local SOCKS5 connections, performs the handshake and CONNECT
/// negotiation with `fast_socks5`, then proxies to whatever target the SOCKS
/// client requested over a fresh direct-tcpip channel — SSH itself has no
/// SOCKS concept, so this is the layer that makes `-D` behave like `ssh -D`.
pub async fn run(
    handle: Arc<Handle<Client>>,
    local_bind: &SocketAddrSpec,
    mut stop_rx: mpsc::Receiver<()>,
) -> Result<()> {
    let listener = TcpListener::bind((local_bind.bind_addr.as_str(), local_bind.port)).await?;
    info!(addr = %local_bind.bind_addr, port = local_bind.port, "dynamic (SOCKS5) forward listening");

    let mut proxies = JoinSet::new();

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _peer) = accepted?;
                let handle = Arc::clone(&handle);
                proxies.spawn(async move {
                    if let Err(err) = proxy_socks_connection(&handle, stream).await {
                        warn!(?err, "dynamic-forward connection proxy failed");
                    }
                });
            }
            _ = stop_rx.recv() => {
                info!("stopping dynamic forward listener");
                break;
            }
        }
    }

    proxies.shutdown().await;
    Ok(())
}

async fn proxy_socks_connection(handle: &Handle<Client>, stream: TcpStream) -> Result<()> {
    let proto = Socks5ServerProtocol::accept_no_auth(stream).await?;
    let (proto, cmd, target_addr) = proto.read_command().await?;

    if !matches!(cmd, Socks5Command::TCPConnect) {
        proto.reply_error(&ReplyError::CommandNotSupported).await?;
        return Ok(());
    }

    let (target_host, target_port) = match target_addr {
        TargetAddr::Ip(addr) => (addr.ip().to_string(), addr.port()),
        TargetAddr::Domain(host, port) => (host, port),
    };

    let channel = match handle
        .channel_open_direct_tcpip(target_host, target_port as u32, "127.0.0.1", 0)
        .await
    {
        Ok(channel) => channel,
        Err(err) => {
            proto.reply_error(&ReplyError::HostUnreachable).await?;
            return Err(err.into());
        }
    };

    // The bound address in a SOCKS5 success reply is informational for most
    // clients; there's no real local socket to report since the actual
    // connection is proxied over the SSH channel, so this is a placeholder.
    let bound: SocketAddr = "0.0.0.0:0".parse().expect("valid placeholder address");
    let mut local = proto.reply_success(bound).await?;
    let mut remote_stream = channel.into_stream();
    copy_bidirectional(&mut local, &mut remote_stream).await?;
    Ok(())
}
