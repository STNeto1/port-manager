use super::schema::{AuthMethod, Profile};

/// A tunnel's connection details flattened out from its `Profile` (and that
/// profile's jump profile, if any) into the shape `ssh::client` actually
/// needs to open a session — resolved once at start time rather than
/// threading profile lookups through the connect/auth/jump code.
pub struct ResolvedConnection {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    pub jump: Option<ResolvedJump>,
}

pub struct ResolvedJump {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
}

pub fn resolve_connection(
    profiles: &[Profile],
    profile_name: &str,
) -> Result<ResolvedConnection, String> {
    let profile = find_profile(profiles, profile_name)?;

    let jump = match &profile.jump {
        Some(jump_name) => {
            let jump_profile = find_profile(profiles, jump_name)?;
            if jump_profile.jump.is_some() {
                return Err(format!(
                    "profile '{jump_name}' has its own jump set; chained (multi-hop) jumps aren't supported yet"
                ));
            }
            Some(ResolvedJump {
                host: jump_profile.host.clone(),
                port: jump_profile.port,
                username: jump_profile.username.clone(),
                auth: jump_profile.auth.clone(),
            })
        }
        None => None,
    };

    Ok(ResolvedConnection {
        host: profile.host.clone(),
        port: profile.port,
        username: profile.username.clone(),
        auth: profile.auth.clone(),
        jump,
    })
}

fn find_profile<'a>(profiles: &'a [Profile], name: &str) -> Result<&'a Profile, String> {
    profiles
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("no profile named '{name}'"))
}
