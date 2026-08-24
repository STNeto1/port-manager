pub mod tui;

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use color_eyre::eyre::{Result, eyre};
use tokio::net::UnixStream;
use tokio::time::sleep;

use uuid::Uuid;

use crate::config::schema::Direction;
use crate::ipc::{
    self,
    protocol::{ClientMessage, ClientRequest, DaemonMessage, ResponsePayload},
};
use crate::model::{TunnelEvent, TunnelState};

pub struct DaemonClient {
    conn: ipc::Conn,
    next_request_id: u64,
}

impl DaemonClient {
    pub async fn call(&mut self, request: ClientRequest) -> Result<ResponsePayload> {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        ipc::send(
            &mut self.conn,
            &ClientMessage {
                request_id,
                request,
            },
        )
        .await?;

        loop {
            let msg: DaemonMessage = ipc::recv(&mut self.conn)
                .await?
                .ok_or_else(|| eyre!("daemon closed the connection"))?;
            match msg {
                DaemonMessage::Response {
                    request_id: rid,
                    result,
                } if rid == request_id => {
                    return result.map_err(|err| eyre!(err));
                }
                // A stray Response for another in-flight request, or an
                // Event arriving on a non-subscribed connection: not
                // expected in normal use, but harmless to skip past.
                DaemonMessage::Response { .. } | DaemonMessage::Event(_) => continue,
            }
        }
    }

    /// Reads the next pushed event on a connection that already sent
    /// `ClientRequest::Subscribe`. Returns `Ok(None)` when the daemon closes
    /// the connection or (unexpectedly) sends a plain response instead.
    pub async fn recv_event(&mut self) -> Result<Option<TunnelEvent>> {
        match ipc::recv::<DaemonMessage>(&mut self.conn).await? {
            Some(DaemonMessage::Event(event)) => Ok(Some(event)),
            Some(DaemonMessage::Response { .. }) | None => Ok(None),
        }
    }
}

pub async fn connect_or_spawn_daemon() -> Result<DaemonClient> {
    let socket_path = ipc::socket_path();

    let stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(_) => {
            spawn_daemon()?;
            wait_for_daemon(&socket_path).await?
        }
    };

    Ok(DaemonClient {
        conn: ipc::framed(stream),
        next_request_id: 0,
    })
}

fn spawn_daemon() -> Result<()> {
    let exe = std::env::current_exe()?;
    let config_dir = ipc::config_dir();
    std::fs::create_dir_all(&config_dir)?;

    let log_path = config_dir.join("daemon.log");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let log_file_err = log_file.try_clone()?;

    std::process::Command::new(exe)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err))
        .spawn()?;

    Ok(())
}

pub async fn list_tunnels() -> Result<()> {
    let mut client = connect_or_spawn_daemon().await?;
    let ResponsePayload::Tunnels(tunnels) = client.call(ClientRequest::ListTunnels).await? else {
        return Err(eyre!("unexpected response to ListTunnels"));
    };

    if tunnels.is_empty() {
        println!("No tunnels configured.");
        return Ok(());
    }

    for snapshot in tunnels {
        let direction = match snapshot.def.direction {
            Direction::Local => "L",
            Direction::Remote => "R",
            Direction::Dynamic => "D",
        };
        let remote = snapshot
            .def
            .remote
            .as_ref()
            .map(|r| format!("{}:{}", r.host, r.port))
            .unwrap_or_else(|| "-".to_string());
        let status = match snapshot.state {
            TunnelState::Stopped => "stopped".to_string(),
            TunnelState::Connecting => "connecting".to_string(),
            TunnelState::Connected { active_connections } => {
                format!("connected ({active_connections} active)")
            }
            TunnelState::Error(err) => format!("error: {err}"),
            TunnelState::Stopping => "stopping".to_string(),
        };
        println!(
            "{name}\t[{direction}]\t{bind_addr}:{bind_port} <-> {remote}\t{status}",
            name = snapshot.def.name,
            bind_addr = snapshot.def.local_bind.bind_addr,
            bind_port = snapshot.def.local_bind.port,
        );
    }

    Ok(())
}

pub async fn start_tunnel(name: &str) -> Result<()> {
    let mut client = connect_or_spawn_daemon().await?;
    let id = resolve_tunnel_id(&mut client, name).await?;
    client.call(ClientRequest::StartTunnel(id)).await?;
    println!("started '{name}'");
    Ok(())
}

pub async fn stop_tunnel(name: &str) -> Result<()> {
    let mut client = connect_or_spawn_daemon().await?;
    let id = resolve_tunnel_id(&mut client, name).await?;
    client.call(ClientRequest::StopTunnel(id)).await?;
    println!("stopped '{name}'");
    Ok(())
}

/// Doesn't auto-spawn: shutting down a daemon that isn't running is a no-op,
/// not a reason to start one just to immediately stop it.
pub async fn shutdown_daemon() -> Result<()> {
    let socket_path = ipc::socket_path();
    let stream = UnixStream::connect(&socket_path)
        .await
        .map_err(|_| eyre!("no running pmanager daemon found"))?;
    let mut client = DaemonClient {
        conn: ipc::framed(stream),
        next_request_id: 0,
    };
    client.call(ClientRequest::ShutdownDaemon).await?;
    println!("daemon shutting down");
    Ok(())
}

async fn resolve_tunnel_id(client: &mut DaemonClient, name: &str) -> Result<Uuid> {
    let ResponsePayload::Tunnels(tunnels) = client.call(ClientRequest::ListTunnels).await? else {
        return Err(eyre!("unexpected response to ListTunnels"));
    };
    tunnels
        .into_iter()
        .find(|t| t.def.name == name)
        .map(|t| t.def.id)
        .ok_or_else(|| eyre!("no tunnel named '{name}'"))
}

async fn wait_for_daemon(socket_path: &Path) -> Result<UnixStream> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        match UnixStream::connect(socket_path).await {
            Ok(stream) => return Ok(stream),
            Err(err) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(eyre!(
                        "failed to connect to daemon after spawning it: {err}"
                    ));
                }
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}
