use std::path::Path;

use color_eyre::eyre::{Result, bail};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

use crate::ipc::{
    self,
    protocol::{ClientMessage, DaemonMessage, ResponsePayload},
};

pub async fn run() -> Result<()> {
    detach_from_terminal();

    let socket_path = ipc::socket_path();
    prepare_socket(&socket_path).await?;

    let listener = UnixListener::bind(&socket_path)?;
    info!(path = %socket_path.display(), "daemon listening");

    loop {
        let (stream, _addr) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream).await {
                warn!(?err, "connection handler exited with error");
            }
        });
    }
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

async fn handle_connection(stream: UnixStream) -> Result<()> {
    let mut conn = ipc::framed(stream);
    while let Some(msg) = ipc::recv::<ClientMessage>(&mut conn).await? {
        let response = DaemonMessage::Response {
            request_id: msg.request_id,
            result: Ok(ResponsePayload::Ack),
        };
        ipc::send(&mut conn, &response).await?;
    }
    Ok(())
}
