use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Notify;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::resolve;
use crate::config::schema::{Profile, TunnelDefinition};
use crate::config::{self, Config};
use crate::ipc::protocol::TunnelSnapshot;
use crate::model::{TunnelEvent, TunnelState};
use crate::ssh;

type CommandReply = oneshot::Sender<Result<(), String>>;

/// Commands the daemon's connection handlers (and, for `TunnelStateChanged`,
/// the per-tunnel ssh tasks themselves) send to the single actor task that
/// owns all tunnel state, so mutations are never raced against each other
/// regardless of how many clients or in-flight ssh tasks are involved.
pub enum DaemonCommand {
    ListTunnels(oneshot::Sender<Vec<TunnelSnapshot>>),
    ListProfiles(oneshot::Sender<Vec<Profile>>),
    AddTunnel(TunnelDefinition, CommandReply),
    UpdateTunnel(TunnelDefinition, CommandReply),
    RemoveTunnel(Uuid, CommandReply),
    StartTunnel(Uuid, CommandReply),
    StopTunnel(Uuid, CommandReply),
    AddProfile(Profile, CommandReply),
    UpdateProfile(Profile, CommandReply),
    RemoveProfile(String, CommandReply),
    ReloadConfig(CommandReply),
    /// Fire-and-forget: a running tunnel's ssh task reporting a state
    /// transition (Connecting/Connected/Error/Stopped/...).
    TunnelStateChanged(Uuid, TunnelState),
    Shutdown(oneshot::Sender<()>),
}

struct TunnelRuntime {
    def: TunnelDefinition,
    state: TunnelState,
    handle: Option<ssh::TunnelHandle>,
}

pub struct DaemonCore {
    config_path: PathBuf,
    profiles: Vec<Profile>,
    tunnels: HashMap<Uuid, TunnelRuntime>,
    events: broadcast::Sender<TunnelEvent>,
    /// Cloned into per-tunnel event-forwarding tasks so an ssh task's
    /// `TunnelEvent`s can be routed back through this same actor as
    /// `TunnelStateChanged` commands, keeping all state mutation
    /// single-threaded.
    self_cmd_tx: mpsc::Sender<DaemonCommand>,
}

impl DaemonCore {
    pub fn new(
        config_path: PathBuf,
        config: Config,
        events: broadcast::Sender<TunnelEvent>,
        self_cmd_tx: mpsc::Sender<DaemonCommand>,
    ) -> Self {
        let profiles = config.profiles.clone();
        Self {
            config_path,
            profiles,
            tunnels: index_by_id(config),
            events,
            self_cmd_tx,
        }
    }

    fn persist(&self) -> Result<(), String> {
        let config = Config {
            profiles: self.profiles.clone(),
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

    /// Stops and drops any handle currently running for `id`. Dropping a
    /// `TunnelHandle` closes its command channel, which the ssh task's
    /// `cmd_rx.recv()` observes as `None` and treats the same as an
    /// explicit `Stop` — so this alone is enough to tear the task down.
    fn stop_running(&mut self, id: Uuid) {
        if let Some(runtime) = self.tunnels.get_mut(&id) {
            runtime.handle = None;
        }
    }

    fn start(&mut self, id: Uuid) -> Result<(), String> {
        let Some(runtime) = self.tunnels.get(&id) else {
            return Err("tunnel not found".to_string());
        };
        let def = runtime.def.clone();
        let conn = resolve::resolve_connection(&self.profiles, &def.profile)?;

        // Starting an already-running tunnel restarts it cleanly rather
        // than leaking the old task or binding the same local port twice.
        self.stop_running(id);

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let handle = ssh::spawn_tunnel(def, conn, event_tx);

        let forward_cmd_tx = self.self_cmd_tx.clone();
        tokio::spawn(async move {
            while let Some(TunnelEvent::StateChanged(event_id, state)) = event_rx.recv().await {
                if forward_cmd_tx
                    .send(DaemonCommand::TunnelStateChanged(event_id, state))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        if let Some(runtime) = self.tunnels.get_mut(&id) {
            runtime.handle = Some(handle);
        }
        Ok(())
    }

    fn stop(&mut self, id: Uuid) -> Result<(), String> {
        let Some(runtime) = self.tunnels.get(&id) else {
            return Err("tunnel not found".to_string());
        };
        if let Some(handle) = &runtime.handle {
            // Best-effort: if the task already exited this just fails
            // silently, which is fine — there's nothing left to stop.
            let _ = handle.cmd_tx.try_send(ssh::TunnelCommand::Stop);
        }
        Ok(())
    }

    fn add_profile(&mut self, profile: Profile) -> Result<(), String> {
        if self.profiles.iter().any(|p| p.name == profile.name) {
            return Err(format!("a profile named '{}' already exists", profile.name));
        }
        let mut candidate = self.profiles.clone();
        candidate.push(profile.clone());
        resolve::resolve_connection(&candidate, &profile.name)?;
        self.profiles = candidate;
        self.persist()
    }

    fn update_profile(&mut self, profile: Profile) -> Result<(), String> {
        let idx = self
            .profiles
            .iter()
            .position(|p| p.name == profile.name)
            .ok_or_else(|| format!("no profile named '{}'", profile.name))?;
        let mut candidate = self.profiles.clone();
        candidate[idx] = profile.clone();
        resolve::resolve_connection(&candidate, &profile.name)?;
        self.profiles = candidate;
        self.persist()
    }

    fn remove_profile(&mut self, name: &str) -> Result<(), String> {
        if self.tunnels.values().any(|t| t.def.profile == name) {
            return Err(format!(
                "profile '{name}' is used by a tunnel; delete or reassign that tunnel first"
            ));
        }
        if self
            .profiles
            .iter()
            .any(|p| p.jump.as_deref() == Some(name))
        {
            return Err(format!(
                "profile '{name}' is used as a jump host by another profile"
            ));
        }
        let before = self.profiles.len();
        self.profiles.retain(|p| p.name != name);
        if self.profiles.len() == before {
            return Err(format!("no profile named '{name}'"));
        }
        self.persist()
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
                    handle: None,
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
            DaemonCommand::ListProfiles(reply) => {
                let _ = reply.send(core.profiles.clone());
            }
            DaemonCommand::AddTunnel(def, reply) => {
                let id = def.id;
                core.tunnels.insert(
                    id,
                    TunnelRuntime {
                        def,
                        state: TunnelState::Stopped,
                        handle: None,
                    },
                );
                let _ = reply.send(core.persist());
            }
            DaemonCommand::UpdateTunnel(def, reply) => {
                let id = def.id;
                if core.tunnels.contains_key(&id) {
                    // Editing a running tunnel stops it; the user restarts
                    // it explicitly to pick up the new definition.
                    core.stop_running(id);
                    core.tunnels.insert(
                        id,
                        TunnelRuntime {
                            def,
                            state: TunnelState::Stopped,
                            handle: None,
                        },
                    );
                    let _ = reply.send(core.persist());
                } else {
                    let _ = reply.send(Err("tunnel not found".to_string()));
                }
            }
            DaemonCommand::RemoveTunnel(id, reply) => {
                core.stop_running(id);
                if core.tunnels.remove(&id).is_some() {
                    let _ = reply.send(core.persist());
                } else {
                    let _ = reply.send(Err("tunnel not found".to_string()));
                }
            }
            DaemonCommand::StartTunnel(id, reply) => {
                let _ = reply.send(core.start(id));
            }
            DaemonCommand::StopTunnel(id, reply) => {
                let _ = reply.send(core.stop(id));
            }
            DaemonCommand::AddProfile(profile, reply) => {
                let _ = reply.send(core.add_profile(profile));
            }
            DaemonCommand::UpdateProfile(profile, reply) => {
                let _ = reply.send(core.update_profile(profile));
            }
            DaemonCommand::RemoveProfile(name, reply) => {
                let _ = reply.send(core.remove_profile(&name));
            }
            DaemonCommand::ReloadConfig(reply) => match config::load(&core.config_path) {
                Ok(config) => {
                    for id in core.tunnels.keys().copied().collect::<Vec<_>>() {
                        core.stop_running(id);
                    }
                    core.profiles = config.profiles.clone();
                    core.tunnels = index_by_id(config);
                    info!("config reloaded from disk");
                    let _ = reply.send(Ok(()));
                }
                Err(err) => {
                    warn!(?err, "failed to reload config");
                    let _ = reply.send(Err(err.to_string()));
                }
            },
            DaemonCommand::TunnelStateChanged(id, state) => {
                core.set_state(id, state);
            }
            DaemonCommand::Shutdown(reply) => {
                let _ = reply.send(());
                shutdown.notify_one();
                break;
            }
        }
    }
}
