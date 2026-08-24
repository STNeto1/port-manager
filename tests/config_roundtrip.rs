use pmanager::config::resolve::resolve_connection;
use pmanager::config::schema::{
    AuthMethod, Config, Direction, HostPort, Profile, SocketAddrSpec, TunnelDefinition,
};
use uuid::Uuid;

fn sample_profile(name: &str, auth: AuthMethod, jump: Option<String>) -> Profile {
    Profile {
        name: name.to_string(),
        host: "bastion.example.com".to_string(),
        port: 22,
        username: "deploy".to_string(),
        auth,
        jump,
    }
}

fn sample_tunnel(direction: Direction, profile: &str) -> TunnelDefinition {
    TunnelDefinition {
        id: Uuid::new_v4(),
        name: "example".to_string(),
        direction,
        profile: profile.to_string(),
        local_bind: SocketAddrSpec {
            bind_addr: "127.0.0.1".to_string(),
            port: 15432,
        },
        remote: Some(HostPort {
            host: "10.0.0.5".to_string(),
            port: 5432,
        }),
        autostart: false,
        enabled: true,
    }
}

#[test]
fn round_trips_every_direction_and_auth_method() {
    let profiles = vec![
        sample_profile(
            "password-profile",
            AuthMethod::Password {
                password: Some("hunter2".to_string()),
            },
            None,
        ),
        sample_profile(
            "key-profile",
            AuthMethod::PrivateKey {
                path: "/home/me/.ssh/id_ed25519".into(),
                passphrase: None,
            },
            None,
        ),
        sample_profile(
            "nixserver",
            AuthMethod::PrivateKey {
                path: "/home/me/.ssh/nixserver_ed25519".into(),
                passphrase: None,
            },
            None,
        ),
        sample_profile(
            "agent-profile",
            AuthMethod::Agent,
            Some("nixserver".to_string()),
        ),
    ];
    let tunnels = vec![
        sample_tunnel(Direction::Local, "password-profile"),
        sample_tunnel(Direction::Remote, "key-profile"),
        sample_tunnel(Direction::Dynamic, "agent-profile"),
    ];
    let config = Config { profiles, tunnels };

    let serialized = toml::to_string_pretty(&config).expect("serialize");
    let deserialized: Config = toml::from_str(&serialized).expect("deserialize");

    assert_eq!(deserialized.profiles.len(), config.profiles.len());
    assert_eq!(deserialized.tunnels.len(), config.tunnels.len());
    for (original, round_tripped) in config.tunnels.iter().zip(deserialized.tunnels.iter()) {
        assert_eq!(original.id, round_tripped.id);
        assert_eq!(original.name, round_tripped.name);
        assert_eq!(original.direction, round_tripped.direction);
        assert_eq!(original.profile, round_tripped.profile);
    }
}

#[test]
fn resolves_profile_through_a_jump_profile() {
    let profiles = vec![
        sample_profile("nixserver", AuthMethod::Agent, None),
        sample_profile("worker", AuthMethod::Agent, Some("nixserver".to_string())),
    ];

    let resolved = resolve_connection(&profiles, "worker").expect("should resolve");
    assert_eq!(resolved.host, "bastion.example.com");
    let jump = resolved.jump.expect("worker should have a jump");
    assert_eq!(jump.host, "bastion.example.com");
}

#[test]
fn rejects_chained_multi_hop_jumps() {
    let profiles = vec![
        sample_profile("a", AuthMethod::Agent, None),
        sample_profile("b", AuthMethod::Agent, Some("a".to_string())),
        sample_profile("c", AuthMethod::Agent, Some("b".to_string())),
    ];

    let result = resolve_connection(&profiles, "c");
    assert!(result.is_err());
}

#[test]
fn rejects_unknown_profile_reference() {
    let profiles = vec![sample_profile("known", AuthMethod::Agent, None)];
    let result = resolve_connection(&profiles, "does-not-exist");
    assert!(result.is_err());
}

#[test]
fn load_creates_default_config_when_missing() {
    let dir = tempfile_dir();
    let path = dir.join("config.toml");

    let config = pmanager::config::load(&path).expect("load should create a default config");
    assert!(config.tunnels.is_empty());
    assert!(config.profiles.is_empty());
    assert!(path.exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_rejects_malformed_toml() {
    let dir = tempfile_dir();
    let path = dir.join("config.toml");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&path, "this is not valid toml [[[").unwrap();

    let result = pmanager::config::load(&path);
    assert!(result.is_err());

    std::fs::remove_dir_all(&dir).ok();
}

fn tempfile_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("pmanager-test-{}", Uuid::new_v4()))
}
