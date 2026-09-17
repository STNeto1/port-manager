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
    let remote = (direction != Direction::Dynamic).then(|| HostPort {
        host: "10.0.0.5".to_string(),
        port: 5432,
    });
    TunnelDefinition {
        id: Uuid::new_v4(),
        name: "example".to_string(),
        direction,
        profile: profile.to_string(),
        local_bind: SocketAddrSpec {
            bind_addr: "127.0.0.1".to_string(),
            port: 15432,
        },
        remote,
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
    assert_eq!(resolved.jumps.len(), 1);
    assert_eq!(resolved.jumps[0].host, "bastion.example.com");
}

fn profile_with_host(name: &str, host: &str, jump: Option<String>) -> Profile {
    Profile {
        host: host.to_string(),
        ..sample_profile(name, AuthMethod::Agent, jump)
    }
}

#[test]
fn resolves_multi_hop_jump_chain_in_dial_order() {
    let profiles = vec![
        profile_with_host("a", "a.example.com", None),
        profile_with_host("b", "b.example.com", Some("a".to_string())),
        profile_with_host("c", "c.example.com", Some("b".to_string())),
    ];

    let resolved = resolve_connection(&profiles, "c").expect("chain should resolve");
    assert_eq!(resolved.jumps.len(), 2);
    // Dial order is nearest-to-daemon first: "a" before "b".
    assert_eq!(resolved.jumps[0].host, "a.example.com");
    assert_eq!(resolved.jumps[1].host, "b.example.com");
}

#[test]
fn rejects_jump_chain_cycles() {
    let profiles = vec![
        sample_profile("a", AuthMethod::Agent, Some("b".to_string())),
        sample_profile("b", AuthMethod::Agent, Some("a".to_string())),
    ];

    match resolve_connection(&profiles, "a") {
        Err(err) => assert!(err.contains("cycle"), "unexpected error: {err}"),
        Ok(_) => panic!("expected a cycle error"),
    }
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
fn compact_config_round_trips_with_stable_ids() {
    let dir = tempfile_dir();
    let path = dir.join("config.toml");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        &path,
        r#"
[profiles.worker]
host = "worker.example.com"
username = "deploy"

[profiles.worker.local]
web = "1337 -> localhost:3000"
postgres = { listen = "0.0.0.0:15432", target = "db.internal:5432", autostart = true }

[profiles.worker.dynamic]
socks = 1080
"#,
    )
    .unwrap();

    let first = pmanager::config::load(&path).expect("compact config should load");
    assert_eq!(first.profiles.len(), 1);
    assert_eq!(first.tunnels.len(), 3);
    assert!(matches!(first.profiles[0].auth, AuthMethod::Agent));
    let first_ids: Vec<_> = first.tunnels.iter().map(|t| t.id).collect();

    pmanager::config::save(&path, &first).expect("compact config should save");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("web = \"1337 -> localhost:3000\""));
    assert!(saved.contains("socks = 1080"));
    assert!(!saved.contains("[[tunnels]]"));

    let second = pmanager::config::load(&path).expect("saved config should reload");
    let second_ids: Vec<_> = second.tunnels.iter().map(|t| t.id).collect();
    assert_eq!(first_ids, second_ids);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn compact_config_rejects_unknown_fields_and_duplicate_tunnel_names() {
    let dir = tempfile_dir();
    let path = dir.join("config.toml");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        &path,
        r#"
[profiles.one]
host = "one.example.com"
username = "deploy"
unknown = true
"#,
    )
    .unwrap();
    assert!(pmanager::config::load(&path).is_err());

    std::fs::write(
        &path,
        r#"
[profiles.one]
host = "one.example.com"
username = "deploy"
[profiles.one.local]
web = "8000 -> localhost:80"

[profiles.two]
host = "two.example.com"
username = "deploy"
[profiles.two.local]
web = "8001 -> localhost:80"
"#,
    )
    .unwrap();
    let error = format!("{:?}", pmanager::config::load(&path).unwrap_err());
    assert!(error.contains("duplicate tunnel name"), "{error}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn legacy_config_loads_and_saves_as_compact_toml() {
    let dir = tempfile_dir();
    let path = dir.join("config.toml");
    std::fs::create_dir_all(&dir).unwrap();
    let config = Config {
        profiles: vec![sample_profile("worker", AuthMethod::Agent, None)],
        tunnels: vec![sample_tunnel(Direction::Local, "worker")],
    };
    std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();

    let loaded = pmanager::config::load(&path).expect("legacy config should load");
    pmanager::config::save(&path, &loaded).expect("migration save should work");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("[profiles.worker.local]"));
    assert!(!saved.contains("[[profiles]]"));

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
