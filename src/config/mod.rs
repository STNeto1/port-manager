pub mod resolve;
pub mod schema;

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::Deserialize;
use uuid::Uuid;

pub use schema::Config;
use schema::{AuthMethod, Direction, HostPort, Profile, SocketAddrSpec, TunnelDefinition};

const DEFAULT_BIND: &str = "127.0.0.1";

/// The compact, human-facing TOML shape. Profiles and tunnels are named by
/// their table/map keys so adding an ordinary forward only takes one line.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    profiles: BTreeMap<String, FileProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileProfile {
    host: String,
    #[serde(default = "default_ssh_port")]
    port: u16,
    username: String,
    #[serde(default)]
    jump: Option<String>,
    #[serde(default)]
    auth: Option<AuthMethod>,
    #[serde(default)]
    local: BTreeMap<String, ForwardValue>,
    #[serde(default)]
    remote: BTreeMap<String, ForwardValue>,
    #[serde(default)]
    dynamic: BTreeMap<String, DynamicValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ForwardValue {
    Short(String),
    Full(ForwardOptions),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ForwardOptions {
    listen: EndpointValue,
    target: String,
    #[serde(default)]
    autostart: bool,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DynamicValue {
    Port(u16),
    Full(DynamicOptions),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DynamicOptions {
    listen: EndpointValue,
    #[serde(default)]
    autostart: bool,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EndpointValue {
    Port(u16),
    Address(String),
}

fn default_ssh_port() -> u16 {
    22
}

fn default_true() -> bool {
    true
}

/// Loads and validates the compact config. The former array-based shape is
/// still accepted so an old file can be loaded and then rewritten safely.
pub fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        let config = Config::default();
        save(path, &config)?;
        return Ok(config);
    }

    let contents = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("reading config file {}", path.display()))?;

    match toml::from_str::<FileConfig>(&contents) {
        Ok(file) => file
            .into_runtime()
            .wrap_err_with(|| format!("validating compact config file {}", path.display())),
        Err(compact_error) => {
            let legacy: Config = toml::from_str(&contents).map_err(|legacy_error| {
                eyre!(
                    "parsing config file {} as compact TOML failed: {compact_error}; parsing it as the legacy format also failed: {legacy_error}",
                    path.display()
                )
            })?;
            validate_runtime(&legacy)
                .wrap_err_with(|| format!("validating legacy config file {}", path.display()))?;
            Ok(legacy)
        }
    }
}

/// Writes the canonical compact representation atomically.
pub fn save(path: &Path, config: &Config) -> Result<()> {
    validate_runtime(config)?;
    let serialized = encode_compact(config)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }

    let tmp_path = tmp_path_for(path);
    std::fs::write(&tmp_path, serialized)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

impl FileConfig {
    fn into_runtime(self) -> Result<Config> {
        let mut profiles = Vec::with_capacity(self.profiles.len());
        let mut tunnels = Vec::new();
        let mut tunnel_names = HashSet::new();

        for (profile_name, file) in self.profiles {
            require_name("profile", &profile_name)?;
            require_nonempty(&format!("profiles.{profile_name}.host"), &file.host)?;
            require_nonempty(&format!("profiles.{profile_name}.username"), &file.username)?;
            require_port(&format!("profiles.{profile_name}.port"), file.port)?;

            for (name, value) in file.local {
                require_unique_tunnel_name(&mut tunnel_names, &name)?;
                let (local_bind, remote, autostart, enabled) =
                    parse_forward(&format!("profiles.{profile_name}.local.{name}"), value)?;
                tunnels.push(tunnel(
                    &profile_name,
                    name,
                    Direction::Local,
                    local_bind,
                    Some(remote),
                    autostart,
                    enabled,
                ));
            }
            for (name, value) in file.remote {
                require_unique_tunnel_name(&mut tunnel_names, &name)?;
                let (local_bind, remote, autostart, enabled) =
                    parse_forward(&format!("profiles.{profile_name}.remote.{name}"), value)?;
                tunnels.push(tunnel(
                    &profile_name,
                    name,
                    Direction::Remote,
                    local_bind,
                    Some(remote),
                    autostart,
                    enabled,
                ));
            }
            for (name, value) in file.dynamic {
                require_unique_tunnel_name(&mut tunnel_names, &name)?;
                let (local_bind, autostart, enabled) =
                    parse_dynamic(&format!("profiles.{profile_name}.dynamic.{name}"), value)?;
                tunnels.push(tunnel(
                    &profile_name,
                    name,
                    Direction::Dynamic,
                    local_bind,
                    None,
                    autostart,
                    enabled,
                ));
            }

            profiles.push(Profile {
                name: profile_name,
                host: file.host,
                port: file.port,
                username: file.username,
                auth: file.auth.unwrap_or(AuthMethod::Agent),
                jump: file.jump,
            });
        }

        let config = Config { profiles, tunnels };
        validate_runtime(&config)?;
        Ok(config)
    }
}

fn parse_forward(
    path: &str,
    value: ForwardValue,
) -> Result<(SocketAddrSpec, HostPort, bool, bool)> {
    match value {
        ForwardValue::Short(value) => {
            let (listen, target) = value
                .split_once("->")
                .ok_or_else(|| eyre!("{path} must use 'listen -> target'"))?;
            Ok((
                parse_listen(path, EndpointValue::Address(listen.trim().to_string()))?,
                parse_target(path, target.trim())?,
                false,
                true,
            ))
        }
        ForwardValue::Full(options) => Ok((
            parse_listen(path, options.listen)?,
            parse_target(path, &options.target)?,
            options.autostart,
            options.enabled,
        )),
    }
}

fn parse_dynamic(path: &str, value: DynamicValue) -> Result<(SocketAddrSpec, bool, bool)> {
    match value {
        DynamicValue::Port(port) => Ok((socket(DEFAULT_BIND, port), false, true)),
        DynamicValue::Full(options) => Ok((
            parse_listen(path, options.listen)?,
            options.autostart,
            options.enabled,
        )),
    }
}

fn parse_listen(path: &str, value: EndpointValue) -> Result<SocketAddrSpec> {
    match value {
        EndpointValue::Port(port) => Ok(socket(DEFAULT_BIND, port)),
        EndpointValue::Address(value) => {
            if let Ok(port) = value.parse::<u16>() {
                return Ok(socket(DEFAULT_BIND, port));
            }
            let (host, port) = split_host_port(path, &value)?;
            Ok(socket(&host, port))
        }
    }
}

fn parse_target(path: &str, value: &str) -> Result<HostPort> {
    let (host, port) = split_host_port(path, value)?;
    Ok(HostPort { host, port })
}

fn split_host_port(path: &str, value: &str) -> Result<(String, u16)> {
    let (host, port) = value
        .rsplit_once(':')
        .ok_or_else(|| eyre!("{path}: expected host:port, got '{value}'"))?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host)
        .trim();
    require_nonempty(path, host)?;
    let port = port
        .trim()
        .parse::<u16>()
        .wrap_err_with(|| format!("{path}: invalid port in '{value}'"))?;
    require_port(path, port)?;
    Ok((host.to_string(), port))
}

fn socket(bind_addr: &str, port: u16) -> SocketAddrSpec {
    SocketAddrSpec {
        bind_addr: bind_addr.to_string(),
        port,
    }
}

fn tunnel(
    profile: &str,
    name: String,
    direction: Direction,
    local_bind: SocketAddrSpec,
    remote: Option<HostPort>,
    autostart: bool,
    enabled: bool,
) -> TunnelDefinition {
    let identity = format!("{profile}\0{name}");
    TunnelDefinition {
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, identity.as_bytes()),
        name,
        direction,
        profile: profile.to_string(),
        local_bind,
        remote,
        autostart,
        enabled,
    }
}

fn validate_runtime(config: &Config) -> Result<()> {
    let mut profile_names = HashSet::new();
    for profile in &config.profiles {
        require_name("profile", &profile.name)?;
        require_nonempty(&format!("profile '{}'.host", profile.name), &profile.host)?;
        require_nonempty(
            &format!("profile '{}'.username", profile.name),
            &profile.username,
        )?;
        require_port(&format!("profile '{}'.port", profile.name), profile.port)?;
        if !profile_names.insert(profile.name.as_str()) {
            return Err(eyre!("duplicate profile name '{}'", profile.name));
        }
    }

    for profile in &config.profiles {
        if let Some(jump) = &profile.jump
            && !profile_names.contains(jump.as_str())
        {
            return Err(eyre!(
                "profile '{}' refers to unknown jump profile '{}'",
                profile.name,
                jump
            ));
        }
        resolve::resolve_connection(&config.profiles, &profile.name).map_err(|e| eyre!(e))?;
    }

    let mut tunnel_names = HashSet::new();
    for def in &config.tunnels {
        require_unique_tunnel_name(&mut tunnel_names, &def.name)?;
        if !profile_names.contains(def.profile.as_str()) {
            return Err(eyre!(
                "tunnel '{}' refers to unknown profile '{}'",
                def.name,
                def.profile
            ));
        }
        require_nonempty(
            &format!("tunnel '{}'.local_bind.bind_addr", def.name),
            &def.local_bind.bind_addr,
        )?;
        require_port(
            &format!("tunnel '{}'.local_bind.port", def.name),
            def.local_bind.port,
        )?;
        match def.direction {
            Direction::Dynamic if def.remote.is_some() => {
                return Err(eyre!(
                    "dynamic tunnel '{}' must not have a target",
                    def.name
                ));
            }
            Direction::Local | Direction::Remote if def.remote.is_none() => {
                return Err(eyre!("tunnel '{}' requires a target", def.name));
            }
            _ => {}
        }
        if let Some(remote) = &def.remote {
            require_nonempty(&format!("tunnel '{}'.target", def.name), &remote.host)?;
            require_port(&format!("tunnel '{}'.target.port", def.name), remote.port)?;
        }
    }
    Ok(())
}

fn require_port(field: &str, port: u16) -> Result<()> {
    if port == 0 {
        Err(eyre!("{field} must be between 1 and 65535"))
    } else {
        Ok(())
    }
}

fn require_name(kind: &str, value: &str) -> Result<()> {
    require_nonempty(&format!("{kind} name"), value)
}

fn require_nonempty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(eyre!("{field} must not be empty"))
    } else {
        Ok(())
    }
}

fn require_unique_tunnel_name(names: &mut HashSet<String>, name: &str) -> Result<()> {
    require_name("tunnel", name)?;
    if !names.insert(name.to_string()) {
        return Err(eyre!("duplicate tunnel name '{name}'"));
    }
    Ok(())
}

fn encode_compact(config: &Config) -> Result<String> {
    let mut output = String::new();
    let mut profiles: Vec<_> = config.profiles.iter().collect();
    profiles.sort_by(|a, b| a.name.cmp(&b.name));

    for (index, profile) in profiles.into_iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        let profile_key = key(&profile.name)?;
        writeln!(output, "[profiles.{profile_key}]")?;
        writeln!(output, "host = {}", string(&profile.host)?)?;
        if profile.port != 22 {
            writeln!(output, "port = {}", profile.port)?;
        }
        writeln!(output, "username = {}", string(&profile.username)?)?;
        if let Some(jump) = &profile.jump {
            writeln!(output, "jump = {}", string(jump)?)?;
        }

        match &profile.auth {
            AuthMethod::Agent => {}
            AuthMethod::Password { password } => {
                writeln!(output, "\n[profiles.{profile_key}.auth]")?;
                writeln!(output, "type = \"password\"")?;
                if let Some(password) = password {
                    writeln!(output, "password = {}", string(password)?)?;
                }
            }
            AuthMethod::PrivateKey { path, passphrase } => {
                writeln!(output, "\n[profiles.{profile_key}.auth]")?;
                writeln!(output, "type = \"private_key\"")?;
                writeln!(output, "path = {}", string(&path.to_string_lossy())?)?;
                if let Some(passphrase) = passphrase {
                    writeln!(output, "passphrase = {}", string(passphrase)?)?;
                }
            }
        }

        for direction in [Direction::Local, Direction::Remote, Direction::Dynamic] {
            let mut defs: Vec<_> = config
                .tunnels
                .iter()
                .filter(|d| d.profile == profile.name && d.direction == direction)
                .collect();
            defs.sort_by(|a, b| a.name.cmp(&b.name));
            if defs.is_empty() {
                continue;
            }
            let section = match direction {
                Direction::Local => "local",
                Direction::Remote => "remote",
                Direction::Dynamic => "dynamic",
            };
            writeln!(output, "\n[profiles.{profile_key}.{section}]")?;
            for def in defs {
                writeln!(
                    output,
                    "{} = {}",
                    key(&def.name)?,
                    encode_tunnel_value(def)?
                )?;
            }
        }
    }

    Ok(output)
}

fn encode_tunnel_value(def: &TunnelDefinition) -> Result<String> {
    let simple_listen = def.local_bind.bind_addr == DEFAULT_BIND;
    let ordinary = def.enabled && !def.autostart;

    if def.direction == Direction::Dynamic && simple_listen && ordinary {
        return Ok(def.local_bind.port.to_string());
    }

    if def.direction != Direction::Dynamic && simple_listen && ordinary {
        let target = def.remote.as_ref().expect("validated target");
        return string(&format!(
            "{} -> {}:{}",
            def.local_bind.port,
            display_host(&target.host),
            target.port
        ));
    }

    let listen = if simple_listen {
        def.local_bind.port.to_string()
    } else {
        string(&format!(
            "{}:{}",
            display_host(&def.local_bind.bind_addr),
            def.local_bind.port
        ))?
    };
    let mut fields = vec![format!("listen = {listen}")];
    if let Some(target) = &def.remote {
        fields.push(format!(
            "target = {}",
            string(&format!("{}:{}", display_host(&target.host), target.port))?
        ));
    }
    if def.autostart {
        fields.push("autostart = true".to_string());
    }
    if !def.enabled {
        fields.push("enabled = false".to_string());
    }
    Ok(format!("{{ {} }}", fields.join(", ")))
}

fn display_host(host: &str) -> String {
    if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn key(value: &str) -> Result<String> {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !value.is_empty()
    {
        Ok(value.to_string())
    } else {
        string(value)
    }
}

fn string(value: &str) -> Result<String> {
    let wrapper = toml::to_string(&BTreeMap::from([("value", value)]))?;
    Ok(wrapper
        .trim()
        .strip_prefix("value = ")
        .expect("single TOML value")
        .to_string())
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let mut tmp = path.to_path_buf();
    tmp.set_extension("toml.tmp");
    tmp
}
