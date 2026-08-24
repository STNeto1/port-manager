pub mod protocol;

use std::path::PathBuf;

use color_eyre::eyre::Result;
use futures_util::{SinkExt, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use tokio::net::UnixStream;
use tokio_util::codec::{Framed, LinesCodec};

pub type Conn = Framed<UnixStream, LinesCodec>;

pub fn config_dir() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME environment variable must be set");
    PathBuf::from(home).join(".config").join("pmanager")
}

pub fn socket_path() -> PathBuf {
    config_dir().join("pmanager.sock")
}

pub fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn framed(stream: UnixStream) -> Conn {
    Framed::new(stream, LinesCodec::new())
}

pub async fn send<T: Serialize>(conn: &mut Conn, msg: &T) -> Result<()> {
    let line = serde_json::to_string(msg)?;
    conn.send(line).await?;
    Ok(())
}

pub async fn recv<T: DeserializeOwned>(conn: &mut Conn) -> Result<Option<T>> {
    match conn.next().await {
        Some(Ok(line)) => Ok(Some(serde_json::from_str(&line)?)),
        Some(Err(err)) => Err(err.into()),
        None => Ok(None),
    }
}
