use color_eyre::eyre::Result;
use tokio::net::UnixStream;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::ipc::{
    self,
    protocol::{ClientMessage, ClientRequest, DaemonMessage, ResponsePayload},
};

use super::core::DaemonCommand;

pub async fn handle(
    stream: UnixStream,
    cmd_tx: mpsc::Sender<DaemonCommand>,
    events: broadcast::Sender<crate::model::TunnelEvent>,
) -> Result<()> {
    let mut conn = ipc::framed(stream);
    while let Some(msg) = ipc::recv::<ClientMessage>(&mut conn).await? {
        if matches!(msg.request, ClientRequest::Subscribe) {
            let response = DaemonMessage::Response {
                request_id: msg.request_id,
                result: Ok(ResponsePayload::Ack),
            };
            ipc::send(&mut conn, &response).await?;
            return stream_events(conn, events).await;
        }

        let result = dispatch(msg.request, &cmd_tx).await;
        let response = DaemonMessage::Response {
            request_id: msg.request_id,
            result,
        };
        ipc::send(&mut conn, &response).await?;
    }
    Ok(())
}

/// Once a connection subscribes it is dedicated to streaming events for the
/// rest of its life — the TUI client always opens a separate connection for
/// this, so no further requests need to be multiplexed on it.
async fn stream_events(
    mut conn: ipc::Conn,
    events: broadcast::Sender<crate::model::TunnelEvent>,
) -> Result<()> {
    let mut rx = events.subscribe();
    loop {
        match rx.recv().await {
            Ok(event) => {
                if ipc::send(&mut conn, &DaemonMessage::Event(event))
                    .await
                    .is_err()
                {
                    return Ok(());
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return Ok(()),
        }
    }
}

async fn dispatch(
    request: ClientRequest,
    cmd_tx: &mpsc::Sender<DaemonCommand>,
) -> Result<ResponsePayload, String> {
    match request {
        ClientRequest::Ping => Ok(ResponsePayload::Ack),
        ClientRequest::ListTunnels => {
            let (tx, rx) = oneshot::channel();
            send_command(cmd_tx, DaemonCommand::ListTunnels(tx)).await?;
            let tunnels = rx
                .await
                .map_err(|_| "daemon core unavailable".to_string())?;
            Ok(ResponsePayload::Tunnels(tunnels))
        }
        ClientRequest::AddTunnel(def) => {
            simple_command(cmd_tx, |reply| DaemonCommand::AddTunnel(def, reply)).await
        }
        ClientRequest::UpdateTunnel(def) => {
            simple_command(cmd_tx, |reply| DaemonCommand::UpdateTunnel(def, reply)).await
        }
        ClientRequest::RemoveTunnel(id) => {
            simple_command(cmd_tx, |reply| DaemonCommand::RemoveTunnel(id, reply)).await
        }
        ClientRequest::StartTunnel(id) => {
            simple_command(cmd_tx, |reply| DaemonCommand::StartTunnel(id, reply)).await
        }
        ClientRequest::StopTunnel(id) => {
            simple_command(cmd_tx, |reply| DaemonCommand::StopTunnel(id, reply)).await
        }
        ClientRequest::ReloadConfig => simple_command(cmd_tx, DaemonCommand::ReloadConfig).await,
        ClientRequest::ShutdownDaemon => {
            let (tx, rx) = oneshot::channel();
            send_command(cmd_tx, DaemonCommand::Shutdown(tx)).await?;
            rx.await
                .map_err(|_| "daemon core unavailable".to_string())?;
            Ok(ResponsePayload::Ack)
        }
        ClientRequest::Subscribe => unreachable!("handled by the caller before dispatch"),
    }
}

async fn send_command(
    cmd_tx: &mpsc::Sender<DaemonCommand>,
    cmd: DaemonCommand,
) -> Result<(), String> {
    cmd_tx
        .send(cmd)
        .await
        .map_err(|_| "daemon core unavailable".to_string())
}

async fn simple_command<F>(
    cmd_tx: &mpsc::Sender<DaemonCommand>,
    build: F,
) -> Result<ResponsePayload, String>
where
    F: FnOnce(oneshot::Sender<Result<(), String>>) -> DaemonCommand,
{
    let (tx, rx) = oneshot::channel();
    send_command(cmd_tx, build(tx)).await?;
    let result = rx
        .await
        .map_err(|_| "daemon core unavailable".to_string())?;
    result.map(|_| ResponsePayload::Ack)
}
