use std::collections::HashMap;
use std::path::PathBuf;

use std::sync::Arc;
use tokio::sync::Notify;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::schema::TunnelDefinition;
use crate::config::{self, Config};
use crate::ipc::protocol::TunnelSnapshot;
use crate::model::{TunnelEvent, TunnelState};

type CommandReply = oneshot::Sender<Result<(), String>>;

/// Commands the daemon's connection handlers send to the single actor task
/// that owns all tunnel state, so mutations are never raced against each
/// other regardless of how many clients are connected.
pub enum DaemonCommand {
    ListTunnels(oneshot::Sender<Vec<TunnelSnapshot>>),
    AddTunnel(TunnelDefinition, CommandReply),
    UpdateTunnel(TunnelDefinition, CommandReply),
    RemoveTunnel(Uuid, CommandReply),
    StartTunnel(Uuid, CommandReply),
    StopTunnel(Uuid, CommandReply),
    ReloadConfig(CommandReply),
    Shutdown(oneshot::Sender<()>),
}

struct TunnelRuntime {
    def: TunnelDefinition,
    state: TunnelState,
}

pub struct DaemonCore {
    config_path: PathBuf,
    tunnels: HashMap<Uuid, TunnelRuntime>,
    events: broadcast::Sender<TunnelEvent>,
}

impl DaemonCore {
    pub fn new(
        config_path: PathBuf,
        config: Config,
        events: broadcast::Sender<TunnelEvent>,
    ) -> Self {
        Self {
            config_path,
            tunnels: index_by_id(config),
            events,
        }
    }

    fn persist(&self) -> Result<(), String> {
        let config = Config {
            tunnels: self.tunnels.values().map(|t| t.def.clone()).collect(),
        };
        config::save(&self.config_path, &config).map_err(|err| err.to_string())
    }

    fn snapshot_all(&self) -> Vec<TunnelSnapshot> {
        // HashMap iteration order isn't stable across calls; sort by name so
        // the TUI's row order (and its selection index) stays put between refreshes.
        let mut snapshots: Vec<_> = self
            .tunnels
            .values()
            .map(|t| TunnelSnapshot {
                def: t.def.clone(),
                state: t.state.clone(),
            })
            .collect();
        snapshots.sort_by(|a, b| a.def.name.cmp(&b.def.name));
        snapshots
    }

    fn set_state(&mut self, id: Uuid, state: TunnelState) {
        if let Some(runtime) = self.tunnels.get_mut(&id) {
            runtime.state = state.clone();
            let _ = self.events.send(TunnelEvent::StateChanged(id, state));
        }
    }
}

fn index_by_id(config: Config) -> HashMap<Uuid, TunnelRuntime> {
    config
        .tunnels
        .into_iter()
        .map(|def| {
            (
                def.id,
                TunnelRuntime {
                    def,
                    state: TunnelState::Stopped,
                },
            )
        })
        .collect()
}

pub async fn run(
    mut cmd_rx: mpsc::Receiver<DaemonCommand>,
    mut core: DaemonCore,
    shutdown: Arc<Notify>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            DaemonCommand::ListTunnels(reply) => {
                let _ = reply.send(core.snapshot_all());
            }
            DaemonCommand::AddTunnel(def, reply) => {
                let id = def.id;
                core.tunnels.insert(
                    id,
                    TunnelRuntime {
                        def,
                        state: TunnelState::Stopped,
                    },
                );
                let _ = reply.send(core.persist());
            }
            DaemonCommand::UpdateTunnel(def, reply) => {
                let id = def.id;
                if core.tunnels.contains_key(&id) {
                    let state = core.tunnels.get(&id).unwrap().state.clone();
                    core.tunnels.insert(id, TunnelRuntime { def, state });
                    let _ = reply.send(core.persist());
                } else {
                    let _ = reply.send(Err("tunnel not found".to_string()));
                }
            }
            DaemonCommand::RemoveTunnel(id, reply) => {
                if core.tunnels.remove(&id).is_some() {
                    let _ = reply.send(core.persist());
                } else {
                    let _ = reply.send(Err("tunnel not found".to_string()));
                }
            }
            DaemonCommand::StartTunnel(id, reply) => {
                if core.tunnels.contains_key(&id) {
                    // No real SSH yet: fake-toggle to Connected so the
                    // event round-trip and status rendering can be verified.
                    core.set_state(
                        id,
                        TunnelState::Connected {
                            active_connections: 0,
                        },
                    );
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err("tunnel not found".to_string()));
                }
            }
            DaemonCommand::StopTunnel(id, reply) => {
                if core.tunnels.contains_key(&id) {
                    core.set_state(id, TunnelState::Stopped);
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err("tunnel not found".to_string()));
                }
            }
            DaemonCommand::ReloadConfig(reply) => match config::load(&core.config_path) {
                Ok(config) => {
                    core.tunnels = index_by_id(config);
                    info!("config reloaded from disk");
                    let _ = reply.send(Ok(()));
                }
                Err(err) => {
                    warn!(?err, "failed to reload config");
                    let _ = reply.send(Err(err.to_string()));
                }
            },
            DaemonCommand::Shutdown(reply) => {
                let _ = reply.send(());
                shutdown.notify_one();
                break;
            }
        }
    }
}
