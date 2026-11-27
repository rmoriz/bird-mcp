use std::path::PathBuf;

use bird_mcp::config::{Config, TransportKind};
use tempfile::TempDir;

fn home() -> TempDir {
    tempfile::tempdir().expect("temp home")
}

#[test]
fn empty_env_gives_one_read_only_local_router() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let config = Config::from_pairs(&[("HOME", &home_path)]).expect("config");

    assert_eq!(config.routers.len(), 1);
    let router = &config.routers[0];
    assert_eq!(router.name, "local");
    assert_eq!(router.transport, TransportKind::Local);
    assert!(router.read_only);
    assert_eq!(router.birdc, "birdc");
    assert_eq!(router.config_file, "/etc/bird/bird.conf");
    assert_eq!(router.timeout.as_secs(), 20);
    assert!(router.socket.is_none());
}

#[test]
fn json_routers_are_parsed() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let json = r#"[
        {"name": "edge1", "transport": "ssh", "host": "router.lab.example.net", "user": "bird",
         "port": 2222, "socket": "/run/bird/bird.ctl", "read_only": false},
        {"name": "local", "socket": "/tmp/bird.ctl"}
    ]"#;

    let config = Config::from_pairs(&[
        ("HOME", &home_path),
        ("BIRD_MCP_ROUTERS", json),
        ("BIRD_MCP_BIRDC", "/usr/local/sbin/birdc"),
    ])
    .expect("config");

    assert_eq!(config.routers.len(), 2);
    let edge = &config.routers[0];
    assert_eq!(edge.name, "edge1");
    assert_eq!(edge.transport, TransportKind::Ssh);
    assert_eq!(edge.host.as_deref(), Some("router.lab.example.net"));
    assert_eq!(edge.user.as_deref(), Some("bird"));
    assert_eq!(edge.port, Some(2222));
    assert_eq!(edge.target(), "bird@router.lab.example.net");
    assert!(!edge.read_only);

    let local = &config.routers[1];
    assert_eq!(local.transport, TransportKind::Local);
    assert_eq!(local.birdc, "/usr/local/sbin/birdc");
    assert_eq!(local.socket.as_deref(), Some("/tmp/bird.ctl"));
    assert!(local.read_only);
}

#[test]
fn json_object_with_routers_key_is_accepted() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let json = r#"{"routers": [{"name": "r1", "host": "r1.example.net"}]}"#;
    let config =
        Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_ROUTERS", json)]).expect("config");
    assert_eq!(config.routers[0].transport, TransportKind::Ssh);
}

#[test]
fn json_must_be_an_array() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let err = Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_ROUTERS", r#"{"a": 1}"#)])
        .unwrap_err();
    assert!(err.to_string().contains("JSON array"));
}

#[test]
fn duplicate_names_are_rejected() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let json = r#"[{"name": "r1"}, {"name": "r1"}]"#;
    let err = Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_ROUTERS", json)]).unwrap_err();
    assert!(err.to_string().contains("duplicate router name 'r1'"));
}

#[test]
fn ssh_router_needs_a_host() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let json = r#"[{"name": "r1", "transport": "ssh"}]"#;
    let err = Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_ROUTERS", json)]).unwrap_err();
    assert!(err.to_string().contains("needs a host"));
}

#[test]
fn unknown_transport_is_rejected() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let json = r#"[{"name": "r1", "transport": "telnet", "host": "x"}]"#;
    let err = Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_ROUTERS", json)]).unwrap_err();
    assert!(err.to_string().contains("unknown transport"));
}

#[test]
fn toml_config_file_is_parsed() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let path = home.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
docs_dir = "/srv/bird/doc"

[[routers]]
name = "edge1"
transport = "ssh"
host = "router.example.net"
read_only = false
timeout_secs = 5
"#,
    )
    .expect("write config");
    let path_string = path.to_string_lossy().to_string();

    let config = Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_CONFIG", &path_string)])
        .expect("config");

    assert_eq!(config.routers.len(), 1);
    assert_eq!(config.routers[0].name, "edge1");
    assert!(!config.routers[0].read_only);
    assert_eq!(config.routers[0].timeout.as_secs(), 5);
    assert_eq!(config.docs_dir, Some(PathBuf::from("/srv/bird/doc")));
}

#[test]
fn missing_config_file_is_an_error() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let missing = home.path().join("nope.toml");
    let missing_string = missing.to_string_lossy().to_string();
    let err = Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_CONFIG", &missing_string)])
        .unwrap_err();
    assert!(err.to_string().contains("does not exist"));
}

#[test]
fn router_timeout_is_clamped() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();

    let config =
        Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_TIMEOUT", "9999")]).expect("config");
    assert_eq!(config.routers[0].timeout.as_secs(), 300);

    let config =
        Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_TIMEOUT", "0")]).expect("config");
    assert_eq!(config.routers[0].timeout.as_secs(), 1);
}

#[test]
fn routers_are_looked_up_by_name() {
    let home = home();
    let home_path = home.path().to_string_lossy().to_string();
    let json = r#"[{"name": "r1"}, {"name": "r2"}]"#;
    let config =
        Config::from_pairs(&[("HOME", &home_path), ("BIRD_MCP_ROUTERS", json)]).expect("config");

    assert_eq!(config.router(Some("r2")).expect("r2").name, "r2");
    assert_eq!(config.router(None).expect("default").name, "r1");
    let err = config.router(Some("nope")).unwrap_err();
    assert!(err.to_string().contains("unknown router 'nope'"));
    assert!(err.to_string().contains("r1, r2"));
}
