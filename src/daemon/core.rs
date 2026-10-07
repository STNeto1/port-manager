use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

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

/// How often the config file's mtime is checked for hand edits.
const CONFIG_POLL_INTERVAL: Duration = Duration::from_secs(1);

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

impl TunnelRuntime {
    fn stopped(def: TunnelDefinition) -> Self {
        Self {
            def,
            state: TunnelState::Stopped,
            handle: None,
        }
    }
}

pub struct DaemonCore {
    config_path: PathBuf,
    /// The config file's mtime as of the daemon's last read or write of it,
    /// so the poll in `run` only reloads on edits made by someone else.
    config_mtime: Option<SystemTime>,
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
            config_mtime: file_mtime(&config_path),
            config_path,
            profiles,
            tunnels: index_by_id(config),
            events,
            self_cmd_tx,
        }
    }

    fn persist(&mut self) -> Result<(), String> {
        let config = Config {
            profiles: self.profiles.clone(),
            tunnels: self.tunnels.values().map(|t| t.def.clone()).collect(),
        };
        config::save(&self.config_path, &config).map_err(|err| err.to_string())?;
        self.config_mtime = file_mtime(&self.config_path);
        Ok(())
    }

    /// Re-reads the config file and applies it as a diff: a tunnel whose
    /// definition and resolved connection are unchanged keeps its runtime
    /// (and so its open connections); a changed one is stopped, the same as
    /// an edit through `UpdateTunnel`; a removed one is torn down.
    fn reload(&mut self) -> Result<(), String> {
        // Recorded before reading, so an edit landing mid-load still differs
        // from it and is picked up by the next poll.
        self.config_mtime = file_mtime(&self.config_path);
        let config = config::load(&self.config_path).map_err(|err| err.to_string())?;

        // Matched by name rather than id: a tunnel added over IPC carries a
        // client-generated id, while the same tunnel read back from the file
        // gets its derived one. The existing id is kept so clients' ids and
        // in-flight ssh tasks' events stay valid.
        let mut previous: HashMap<String, TunnelRuntime> = self
            .tunnels
            .drain()
            .map(|(_, runtime)| (runtime.def.name.clone(), runtime))
            .collect();
        for mut def in config.tunnels {
            let runtime = match previous.remove(&def.name) {
                Some(old) => {
                    def.id = old.def.id;
                    let same_connection =
                        resolve::resolve_connection(&self.profiles, &old.def.profile).ok()
                            == resolve::resolve_connection(&config.profiles, &def.profile).ok();
                    if old.def == def && same_connection {
                        old
                    } else {
                        TunnelRuntime::stopped(def)
                    }
                }
                None => TunnelRuntime::stopped(def),
            };
            self.tunnels.insert(runtime.def.id, runtime);
        }
        self.profiles = config.profiles;
        Ok(())
    }

    fn reload_if_changed(&mut self) {
        // A missing file is skipped rather than reloaded: some editors save
        // by moving the original aside first, and `config::load` would
        // answer that gap by writing an empty default config over the edit.
        let Some(mtime) = file_mtime(&self.config_path) else {
            return;
        };
        if self.config_mtime == Some(mtime) {
            return;
        }
        // Wait for the file to sit untouched for one interval so a save
        // still in progress isn't read half-written.
        if mtime.elapsed().is_ok_and(|age| age < CONFIG_POLL_INTERVAL) {
            return;
        }
        match self.reload() {
            Ok(()) => info!("config file changed on disk, reloaded"),
            Err(err) => warn!(%err, "config file changed on disk but failed to reload"),
        }
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

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
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
    let mut config_poll = tokio::time::interval(CONFIG_POLL_INTERVAL);
    config_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let cmd = tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(cmd) => cmd,
                None => break,
            },
            _ = config_poll.tick() => {
                core.reload_if_changed();
                continue;
            }
        };
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
            DaemonCommand::ReloadConfig(reply) => {
                let result = core.reload();
                match &result {
                    Ok(()) => info!("config reloaded from disk"),
                    Err(err) => warn!(%err, "failed to reload config"),
                }
                let _ = reply.send(result);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
[profiles.box]
host = "10.0.0.1"
username = "deploy"

[profiles.box.local]
db = "5432 -> localhost:5432"
web = "8080 -> localhost:80"
"#;

    fn state_of(core: &DaemonCore, name: &str) -> TunnelState {
        let runtime = core.tunnels.values().find(|t| t.def.name == name);
        runtime.expect("tunnel should exist").state.clone()
    }

    #[tokio::test]
    async fn reload_keeps_unchanged_tunnels_and_stops_changed_ones() {
        let dir = std::env::temp_dir().join(format!("pmanager-core-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, CONFIG).unwrap();

        let (events, _events_rx) = broadcast::channel(16);
        let (cmd_tx, _cmd_rx) = mpsc::channel(16);
        let mut core = DaemonCore::new(path.clone(), config::load(&path).unwrap(), events, cmd_tx);
        let ids: Vec<Uuid> = core.tunnels.keys().copied().collect();
        for runtime in core.tunnels.values_mut() {
            runtime.state = TunnelState::Connected {
                active_connections: 1,
            };
        }

        // Unrelated additions leave both running tunnels alone.
        let added = format!("{CONFIG}cache = \"6379 -> localhost:6379\"\n");
        std::fs::write(&path, &added).unwrap();
        core.reload().unwrap();
        assert_eq!(core.tunnels.len(), 3);
        assert!(matches!(
            state_of(&core, "db"),
            TunnelState::Connected { .. }
        ));
        assert!(matches!(
            state_of(&core, "web"),
            TunnelState::Connected { .. }
        ));
        assert!(matches!(state_of(&core, "cache"), TunnelState::Stopped));

        // Editing one tunnel stops only that one, and ids stay stable.
        std::fs::write(&path, added.replace("8080", "8081")).unwrap();
        core.reload().unwrap();
        assert!(matches!(
            state_of(&core, "db"),
            TunnelState::Connected { .. }
        ));
        assert!(matches!(state_of(&core, "web"), TunnelState::Stopped));
        assert!(ids.iter().all(|id| core.tunnels.contains_key(id)));

        // A profile change affects every tunnel connecting through it.
        std::fs::write(&path, added.replace("10.0.0.1", "10.0.0.2")).unwrap();
        core.reload().unwrap();
        assert!(matches!(state_of(&core, "db"), TunnelState::Stopped));

        // A removed tunnel is dropped.
        std::fs::write(&path, CONFIG).unwrap();
        core.reload().unwrap();
        assert_eq!(core.tunnels.len(), 2);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
