use pmanager::config::schema::{
    AuthMethod, Config, Direction, HostPort, JumpHost, SocketAddrSpec, TunnelDefinition,
};
use uuid::Uuid;

fn sample_tunnel(direction: Direction, auth: AuthMethod, jump: Option<JumpHost>) -> TunnelDefinition {
    TunnelDefinition {
        id: Uuid::new_v4(),
        name: "example".to_string(),
        direction,
        host: "bastion.example.com".to_string(),
        port: 22,
        username: "deploy".to_string(),
        auth,
        jump,
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
    let tunnels = vec![
        sample_tunnel(
            Direction::Local,
            AuthMethod::Password {
                password: Some("hunter2".to_string()),
            },
            None,
        ),
        sample_tunnel(
            Direction::Remote,
            AuthMethod::PrivateKey {
                path: "/home/me/.ssh/id_ed25519".into(),
                passphrase: None,
            },
            None,
        ),
        sample_tunnel(
            Direction::Dynamic,
            AuthMethod::Agent,
            Some(JumpHost {
                host: "192.168.1.50".to_string(),
                port: 22,
                username: "germano".to_string(),
                auth: AuthMethod::PrivateKey {
                    path: "/home/me/.ssh/nixserver_ed25519".into(),
                    passphrase: None,
                },
            }),
        ),
    ];
    let config = Config { tunnels };

    let serialized = toml::to_string_pretty(&config).expect("serialize");
    let deserialized: Config = toml::from_str(&serialized).expect("deserialize");

    assert_eq!(deserialized.tunnels.len(), config.tunnels.len());
    for (original, round_tripped) in config.tunnels.iter().zip(deserialized.tunnels.iter()) {
        assert_eq!(original.id, round_tripped.id);
        assert_eq!(original.name, round_tripped.name);
        assert_eq!(original.direction, round_tripped.direction);
    }
}

#[test]
fn load_creates_default_config_when_missing() {
    let dir = tempfile_dir();
    let path = dir.join("config.toml");

    let config = pmanager::config::load(&path).expect("load should create a default config");
    assert!(config.tunnels.is_empty());
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
    let dir = std::env::temp_dir().join(format!("pmanager-test-{}", Uuid::new_v4()));
    dir
}
