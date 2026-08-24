mod connection;
mod core;

use std::path::Path;
use std::sync::Arc;

use color_eyre::eyre::{Result, bail};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Notify, broadcast, mpsc, oneshot};
use tracing::{info, warn};

use crate::config;
use crate::ipc;

pub async fn run() -> Result<()> {
    detach_from_terminal();

    let socket_path = ipc::socket_path();
    prepare_socket(&socket_path).await?;

    let config_path = ipc::config_file_path();
    let loaded_config = config::load(&config_path)?;
    info!(tunnels = loaded_config.tunnels.len(), path = %config_path.display(), "config loaded");

    let autostart_ids: Vec<_> = loaded_config
        .tunnels
        .iter()
        .filter(|t| t.autostart)
        .map(|t| t.id)
        .collect();

    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    let (events_tx, _events_rx) = broadcast::channel(256);
    let shutdown = Arc::new(Notify::new());

    let daemon_core = core::DaemonCore::new(
        config_path,
        loaded_config,
        events_tx.clone(),
        cmd_tx.clone(),
    );
    tokio::spawn(core::run(cmd_rx, daemon_core, Arc::clone(&shutdown)));

    for id in autostart_ids {
        let cmd_tx = cmd_tx.clone();
        tokio::spawn(async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            if cmd_tx
                .send(core::DaemonCommand::StartTunnel(id, reply_tx))
                .await
                .is_err()
            {
                return;
            }
            if let Ok(Err(err)) = reply_rx.await {
                warn!(tunnel_id = %id, ?err, "autostart failed");
            }
        });
    }

    let listener = UnixListener::bind(&socket_path)?;
    info!(path = %socket_path.display(), "daemon listening");

    let ctrl_c_shutdown = Arc::clone(&shutdown);
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            info!("received Ctrl+C, shutting down");
            ctrl_c_shutdown.notify_one();
        }
    });

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (stream, _addr) = accept_result?;
                let cmd_tx = cmd_tx.clone();
                let events_tx = events_tx.clone();
                tokio::spawn(async move {
                    if let Err(err) = connection::handle(stream, cmd_tx, events_tx).await {
                        warn!(?err, "connection handler exited with error");
                    }
                });
            }
            _ = shutdown.notified() => {
                info!("shutdown requested, stopping daemon");
                break;
            }
        }
    }

    let _ = tokio::fs::remove_file(&socket_path).await;
    Ok(())
}

/// Starts a new session so the daemon survives its launching terminal
/// closing (e.g. the TUI's tmux pane exiting) instead of receiving SIGHUP.
/// Fails harmlessly with EPERM if this process is already a session leader.
fn detach_from_terminal() {
    unsafe {
        libc::setsid();
    }
}

async fn prepare_socket(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await?;
        }
    }

    if path.exists() {
        // Stale socket check: nothing is listening if a connection attempt fails.
        if UnixStream::connect(path).await.is_err() {
            tokio::fs::remove_file(path).await?;
        } else {
            bail!("a pmanager daemon is already running at {}", path.display());
        }
    }

    Ok(())
}
