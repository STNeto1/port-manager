use std::sync::Arc;

use color_eyre::eyre::{Result, bail, eyre};
use russh::client::{AuthResult, Handle};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key};

use crate::config::schema::AuthMethod;

use super::client::Client;

pub async fn authenticate(
    handle: &mut Handle<Client>,
    username: &str,
    auth: &AuthMethod,
) -> Result<()> {
    let result = match auth {
        AuthMethod::Password { password } => {
            let password = password
                .clone()
                .ok_or_else(|| eyre!("password auth selected but no password is stored"))?;
            handle.authenticate_password(username, password).await?
        }
        AuthMethod::PrivateKey { path, passphrase } => {
            let key = load_secret_key(path, passphrase.as_deref())
                .map_err(|err| eyre!("loading private key {}: {err}", path.display()))?;
            let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
            handle
                .authenticate_publickey(
                    username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
                )
                .await?
        }
        AuthMethod::Agent => authenticate_via_agent(handle, username).await?,
    };

    match result {
        AuthResult::Success => Ok(()),
        AuthResult::Failure { .. } => bail!("SSH authentication failed for user '{username}'"),
    }
}

async fn authenticate_via_agent(handle: &mut Handle<Client>, username: &str) -> Result<AuthResult> {
    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|err| eyre!("connecting to ssh-agent: {err}"))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|err| eyre!("listing ssh-agent identities: {err}"))?;

    for identity in identities {
        let AgentIdentity::PublicKey { key, .. } = identity else {
            continue;
        };
        let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
        if let Ok(AuthResult::Success) = handle
            .authenticate_publickey_with(username, key, hash_alg, &mut agent)
            .await
        {
            return Ok(AuthResult::Success);
        }
    }

    Ok(AuthResult::Failure {
        remaining_methods: russh::MethodSet::empty(),
        partial_success: false,
    })
}
