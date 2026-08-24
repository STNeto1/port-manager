pub mod auth;
pub mod client;
pub mod dynamic_forward;
pub mod jump;
pub mod local_forward;
pub mod remote_forward;

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::config::resolve::ResolvedConnection;
use crate::config::schema::{Direction, TunnelDefinition};
use crate::model::{TunnelEvent, TunnelState};

pub struct TunnelHandle {
    pub cmd_tx: mpsc::Sender<TunnelCommand>,
    pub join: JoinHandle<()>,
}

pub enum TunnelCommand {
    Stop,
}

/// Spawns the tokio task that owns one tunnel's SSH session and forwarding
/// for its whole lifetime: connects (through a jump host first if
/// configured), authenticates, starts the direction-specific forwarder, and
/// reports every state transition on `event_tx` so the daemon core (and, via
/// the broadcast channel, subscribed TUI clients) can follow along live.
/// `conn` is the tunnel's profile already resolved to concrete connection
/// details — resolving happens once, before the task is spawned.
pub fn spawn_tunnel(
    def: TunnelDefinition,
    conn: ResolvedConnection,
    event_tx: mpsc::UnboundedSender<TunnelEvent>,
) -> TunnelHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(4);
    let join = tokio::spawn(run_tunnel_task(def, conn, event_tx, cmd_rx));
    TunnelHandle { cmd_tx, join }
}

async fn run_tunnel_task(
    def: TunnelDefinition,
    conn: ResolvedConnection,
    event_tx: mpsc::UnboundedSender<TunnelEvent>,
    mut cmd_rx: mpsc::Receiver<TunnelCommand>,
) {
    let id = def.id;
    let send_state = |state: TunnelState| {
        let _ = event_tx.send(TunnelEvent::StateChanged(id, state));
    };

    send_state(TunnelState::Connecting);

    let connection = match client::connect(id, &def, &conn).await {
        Ok(connection) => connection,
        Err(err) => {
            warn!(tunnel_id = %id, ?err, "failed to connect tunnel");
            send_state(TunnelState::Error(err.to_string()));
            return;
        }
    };

    send_state(TunnelState::Connected {
        active_connections: 0,
    });

    let target = Arc::new(connection.target);
    let (stop_tx, stop_rx) = mpsc::channel::<()>(1);

    let mut forward_task: JoinHandle<color_eyre::eyre::Result<()>> = tokio::spawn({
        let target = Arc::clone(&target);
        let def = def.clone();
        async move {
            match def.direction {
                Direction::Local => {
                    let remote = def.remote.clone().expect(
                        "Local direction always has a remote target (form/config validated this)",
                    );
                    local_forward::run(target, &def.local_bind, &remote, stop_rx).await
                }
                Direction::Remote => remote_forward::run(target, &def.local_bind, stop_rx).await,
                Direction::Dynamic => dynamic_forward::run(target, &def.local_bind, stop_rx).await,
            }
        }
    });

    tokio::select! {
        cmd = cmd_rx.recv() => {
            // A Stop command, or the sender being dropped (e.g. the daemon
            // removed/replaced this tunnel's definition) — either way, stop.
            let _ = cmd;
            send_state(TunnelState::Stopping);
            let _ = stop_tx.send(()).await;
            let _ = (&mut forward_task).await;
        }
        result = &mut forward_task => {
            report_forward_result(id, result, &event_tx);
            return;
        }
    }

    send_state(TunnelState::Stopped);
}

fn report_forward_result(
    id: uuid::Uuid,
    result: Result<color_eyre::eyre::Result<()>, tokio::task::JoinError>,
    event_tx: &mpsc::UnboundedSender<TunnelEvent>,
) {
    let state = match result {
        Ok(Ok(())) => TunnelState::Stopped,
        Ok(Err(err)) => {
            warn!(tunnel_id = %id, ?err, "tunnel forwarding task failed");
            TunnelState::Error(err.to_string())
        }
        Err(join_err) => {
            warn!(tunnel_id = %id, ?join_err, "tunnel forwarding task panicked");
            TunnelState::Error("forwarding task panicked".to_string())
        }
    };
    let _ = event_tx.send(TunnelEvent::StateChanged(id, state));
}
