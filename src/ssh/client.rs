use std::sync::Arc;

use color_eyre::eyre::Result;
use russh::Channel;
use russh::client::{self, ChannelOpenHandle, Msg, Session};
use russh::keys::PublicKeyOrCertificate;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::resolve::ResolvedConnection;
use crate::config::schema::{Direction, HostPort, TunnelDefinition};

use super::auth;

pub struct Client {
    tunnel_id: Uuid,
    /// The host:port this session is actually verifying against — for a
    /// jump-chained target session this is the *target*'s address, not the
    /// jump host's, since that's the identity known_hosts should pin.
    host: String,
    port: u16,
    /// Set only for Remote-direction tunnels: where to proxy each inbound
    /// `forwarded-tcpip` channel the server opens on us.
    remote_forward_target: Option<HostPort>,
}

impl Client {
    pub fn new(tunnel_id: Uuid, host: impl Into<String>, port: u16) -> Self {
        Self {
            tunnel_id,
            host: host.into(),
            port,
            remote_forward_target: None,
        }
    }

    pub fn with_remote_forward_target(
        tunnel_id: Uuid,
        host: impl Into<String>,
        port: u16,
        target: HostPort,
    ) -> Self {
        Self {
            tunnel_id,
            host: host.into(),
            port,
            remote_forward_target: Some(target),
        }
    }

    /// Accepts a host certificate only if it's signed by a CA trusted for
    /// this host (an `@cert-authority` entry in `~/.ssh/known_hosts`) and
    /// its principals include this host. `Certificate::validate` already
    /// checks the CA signature and the validity time window; the
    /// principals check is left to the caller by design (see its docs), so
    /// it's done here explicitly.
    fn check_server_certificate(&self, cert: &russh::keys::Certificate) -> bool {
        let trusted = super::ca_trust::trusted_fingerprints_for_host(&self.host, self.port);
        if trusted.is_empty() {
            warn!(
                tunnel_id = %self.tunnel_id,
                host = %self.host,
                "REJECTING connection: host presented a certificate but no @cert-authority entry in ~/.ssh/known_hosts matches this host"
            );
            return false;
        }

        if let Err(err) = cert.validate(trusted.iter()) {
            warn!(
                tunnel_id = %self.tunnel_id,
                host = %self.host,
                ?err,
                "REJECTING connection: host certificate failed CA/validity-window validation"
            );
            return false;
        }

        let principals = cert.valid_principals();
        if !principals.is_empty() && !principals.iter().any(|p| p == &self.host) {
            warn!(
                tunnel_id = %self.tunnel_id,
                host = %self.host,
                ?principals,
                "REJECTING connection: host certificate's principals don't include this host"
            );
            return false;
        }

        info!(
            tunnel_id = %self.tunnel_id,
            host = %self.host,
            "host certificate verified against a trusted CA"
        );
        true
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    /// Verifies a plain host key against the user's own `~/.ssh/known_hosts`
    /// (the standard location `russh::keys::check_known_hosts` reads), the
    /// same file the system `ssh` client uses — so hosts already trusted
    /// via a normal `ssh` connection are trusted here too. An unknown host
    /// is trusted on first use and then recorded, so later connections pin
    /// it: this is "accept-new" semantics (like
    /// `ssh -o StrictHostKeyChecking=accept-new`), not full interactive
    /// prompting, since a headless daemon has no terminal to prompt on. A
    /// host presenting a *different* key than the one on record is always
    /// rejected — that's the actual MITM case this exists to catch.
    ///
    /// A host presenting a *certificate* instead is verified against
    /// `@cert-authority` lines in the same `known_hosts` file (see
    /// `ssh::ca_trust`) rather than through TOFU — a certificate is only
    /// trustworthy because a CA signed it, so there's no "first use" to
    /// pin.
    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key,
            PublicKeyOrCertificate::Certificate(cert) => {
                return Ok(self.check_server_certificate(cert));
            }
        };

        match russh::keys::check_known_hosts(&self.host, self.port, key) {
            Ok(true) => Ok(true),
            Ok(false) => {
                match russh::keys::known_hosts::learn_known_hosts(&self.host, self.port, key) {
                    Ok(()) => info!(
                        tunnel_id = %self.tunnel_id,
                        host = %self.host,
                        port = self.port,
                        "trusting new host key on first connection (recorded to ~/.ssh/known_hosts)"
                    ),
                    Err(err) => warn!(
                        tunnel_id = %self.tunnel_id,
                        host = %self.host,
                        ?err,
                        "failed to record new host key in known_hosts; accepting this connection anyway"
                    ),
                }
                Ok(true)
            }
            Err(russh::keys::Error::KeyChanged { line }) => {
                warn!(
                    tunnel_id = %self.tunnel_id,
                    host = %self.host,
                    port = self.port,
                    known_hosts_line = line,
                    "REJECTING connection: host key differs from the one recorded in known_hosts (possible MITM, or the host was reinstalled/re-keyed)"
                );
                Ok(false)
            }
            Err(err) => {
                warn!(tunnel_id = %self.tunnel_id, host = %self.host, ?err, "known_hosts check failed, rejecting connection");
                Ok(false)
            }
        }
    }

    /// Only ever fires for a Remote-direction tunnel's target session,
    /// since only those call `tcpip_forward`. Accepts every inbound
    /// forwarded connection and hands it off to a background task that
    /// proxies it to `remote_forward_target`.
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        _connected_address: &str,
        _connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;

        let tunnel_id = self.tunnel_id;
        if let Some(target) = self.remote_forward_target.clone() {
            tokio::spawn(async move {
                if let Err(err) =
                    super::remote_forward::proxy_forwarded_channel(channel, target).await
                {
                    warn!(tunnel_id = %tunnel_id, ?err, "remote-forward connection proxy failed");
                }
            });
        }

        Ok(())
    }
}

/// An authenticated session to a tunnel's target host, transparently hopping
/// through `conn.jumps` first if any are set. Each jump's own session must
/// be kept alive for as long as the target session is used — the channel it
/// opened is what the next hop's (or the target's) stream runs over — so
/// every intermediate handle is held here rather than only returning the
/// target one.
pub struct Connection {
    pub target: client::Handle<Client>,
    _jump_guards: Vec<client::Handle<Client>>,
}

pub async fn connect(
    tunnel_id: Uuid,
    def: &TunnelDefinition,
    conn: &ResolvedConnection,
) -> Result<Connection> {
    // `Config::default()`'s `preferred.host_key_certificates` is empty, so
    // without this a server is never even offered the chance to present a
    // host certificate — mirroring the default plain-key algorithm list is
    // what lets `check_server_key`'s certificate branch actually run.
    let mut config = client::Config {
        nodelay: true,
        ..Default::default()
    };
    config.preferred.host_key_certificates = config.preferred.key.clone();
    let config = Arc::new(config);

    let target_client = match def.direction {
        Direction::Remote => {
            let target = def.remote.clone().expect(
                "Remote direction always has a local forward target (form/config validated this)",
            );
            Client::with_remote_forward_target(tunnel_id, &conn.host, conn.port, target)
        }
        Direction::Local | Direction::Dynamic => Client::new(tunnel_id, &conn.host, conn.port),
    };

    let connection = if conn.jumps.is_empty() {
        let mut handle =
            client::connect(config, (conn.host.as_str(), conn.port), target_client).await?;
        auth::authenticate(&mut handle, &conn.username, &conn.auth).await?;
        Connection {
            target: handle,
            _jump_guards: Vec::new(),
        }
    } else {
        let (jump_handles, mut target_handle) = super::jump::connect_through(
            tunnel_id,
            &conn.jumps,
            &conn.host,
            conn.port,
            config,
            target_client,
        )
        .await?;
        auth::authenticate(&mut target_handle, &conn.username, &conn.auth).await?;
        Connection {
            target: target_handle,
            _jump_guards: jump_handles,
        }
    };

    Ok(connection)
}
