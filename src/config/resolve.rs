use super::schema::{AuthMethod, Profile};

/// A tunnel's connection details flattened out from its `Profile` (and the
/// chain of jump profiles leading to it, if any) into the shape
/// `ssh::client` actually needs to open a session — resolved once at start
/// time rather than threading profile lookups through the connect/auth/jump
/// code.
pub struct ResolvedConnection {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    /// Ordered nearest-to-daemon first, target-adjacent last — the order
    /// `ssh::jump::connect_through` dials them in.
    pub jumps: Vec<ResolvedJump>,
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

    // Walk `jump` links from the target backwards to the entry point,
    // collecting hops target-adjacent-first, then reverse so `jumps` ends
    // up ordered nearest-to-daemon-first (the order they're dialed in).
    let mut jumps = Vec::new();
    let mut visited = vec![profile_name.to_string()];
    let mut next = profile.jump.clone();
    while let Some(jump_name) = next {
        if visited.contains(&jump_name) {
            return Err(format!("jump chain has a cycle at profile '{jump_name}'"));
        }
        let jump_profile = find_profile(profiles, &jump_name)?;
        jumps.push(ResolvedJump {
            host: jump_profile.host.clone(),
            port: jump_profile.port,
            username: jump_profile.username.clone(),
            auth: jump_profile.auth.clone(),
        });
        visited.push(jump_name);
        next = jump_profile.jump.clone();
    }
    jumps.reverse();

    Ok(ResolvedConnection {
        host: profile.host.clone(),
        port: profile.port,
        username: profile.username.clone(),
        auth: profile.auth.clone(),
        jumps,
    })
}

fn find_profile<'a>(profiles: &'a [Profile], name: &str) -> Result<&'a Profile, String> {
    profiles
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("no profile named '{name}'"))
}
