use std::borrow::Cow;
use std::path::PathBuf;

use russh::keys::{HashAlg, PublicKey, ssh_key::Fingerprint};

/// Trusted CA fingerprints for `host:port`, read from `@cert-authority`
/// lines in `~/.ssh/known_hosts` — the same marker OpenSSH's own
/// known_hosts format uses for CA trust, reusing this project's existing
/// approach of piggybacking on the standard `ssh` files (see
/// `ssh::client`'s plain-host-key verification) rather than inventing a
/// new pmanager-specific config field for it.
pub fn trusted_fingerprints_for_host(host: &str, port: u16) -> Vec<Fingerprint> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let path = PathBuf::from(home).join(".ssh").join("known_hosts");
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

    let host_port = if port == 22 {
        Cow::Borrowed(host)
    } else {
        Cow::Owned(format!("[{host}]:{port}"))
    };

    contents
        .lines()
        .filter_map(|line| line.strip_prefix("@cert-authority "))
        .filter_map(|rest| {
            let mut parts = rest.splitn(3, ' ');
            let patterns = parts.next()?;
            let keytype = parts.next()?;
            let key_b64 = parts.next()?.split_whitespace().next()?;
            if !host_matches_patterns(&host_port, patterns) {
                return None;
            }
            PublicKey::from_openssh(&format!("{keytype} {key_b64}")).ok()
        })
        .map(|key| key.fingerprint(HashAlg::Sha256))
        .collect()
}

/// A simplified version of OpenSSH's known_hosts pattern matching: an exact
/// `host` or `[host]:port` match, or a bare `*` wildcard. Doesn't support
/// full glob patterns (e.g. `*.example.com`) or negation — a real gap for
/// large CA-trust setups, but enough for the common single-host or
/// trust-everything cases.
fn host_matches_patterns(host_port: &str, patterns: &str) -> bool {
    patterns.split(',').any(|p| p == "*" || p == host_port)
}
