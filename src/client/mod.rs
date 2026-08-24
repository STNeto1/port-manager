pub mod tui;

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use color_eyre::eyre::{Result, eyre};
use tokio::net::UnixStream;
use tokio::time::sleep;

use crate::ipc::{
    self,
    protocol::{ClientMessage, ClientRequest, DaemonMessage, ResponsePayload},
};

pub struct DaemonClient {
    conn: ipc::Conn,
    next_request_id: u64,
}

impl DaemonClient {
    pub async fn call(&mut self, request: ClientRequest) -> Result<ResponsePayload> {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        ipc::send(&mut self.conn, &ClientMessage { request_id, request }).await?;

        loop {
            let msg: DaemonMessage = ipc::recv(&mut self.conn)
                .await?
                .ok_or_else(|| eyre!("daemon closed the connection"))?;
            match msg {
                DaemonMessage::Response { request_id: rid, result } if rid == request_id => {
                    return result.map_err(|err| eyre!(err));
                }
                DaemonMessage::Response { .. } => continue,
            }
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

async fn wait_for_daemon(socket_path: &Path) -> Result<UnixStream> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        match UnixStream::connect(socket_path).await {
            Ok(stream) => return Ok(stream),
            Err(err) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(eyre!("failed to connect to daemon after spawning it: {err}"));
                }
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}
