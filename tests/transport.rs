use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use bird_mcp::config::{Router, TransportKind};
use bird_mcp::error::ToolError;
use bird_mcp::transport::shell_quote;

fn script(dir: &Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write script");
    let mut permissions = std::fs::metadata(&path).expect("stat").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("chmod");
    path.to_string_lossy().to_string()
}

fn local_router(binary: &str, timeout_secs: u64) -> Router {
    Router {
        name: "test".to_string(),
        transport: TransportKind::Local,
        host: None,
        user: None,
        port: None,
        birdc: binary.to_string(),
        socket: Some("/tmp/bird.ctl".to_string()),
        config_file: "/etc/bird/bird.conf".to_string(),
        read_only: true,
        timeout: Duration::from_secs(timeout_secs),
        ssh_bin: "ssh".to_string(),
    }
}

#[tokio::test]
async fn local_birdc_receives_socket_and_command() {
    let dir = tempfile::tempdir().expect("temp dir");
    let birdc = script(dir.path(), "birdc", r#"printf '%s\n' "$@""#);
    let router = local_router(&birdc, 5);

    let output = router.run("show status").await.expect("run");
    assert!(output.success());
    let args: Vec<&str> = output.stdout.lines().collect();
    assert_eq!(args, vec!["-s", "/tmp/bird.ctl", "show status"]);
}

#[tokio::test]
async fn failing_command_reports_exit_code_and_stderr() {
    let dir = tempfile::tempdir().expect("temp dir");
    let birdc = script(
        dir.path(),
        "birdc",
        "echo 'syntax error, unexpected CF_SYM_UNDEFINED' >&2\nexit 1",
    );
    let router = local_router(&birdc, 5);

    let output = router.run("show nonsense").await.expect("run");
    assert!(!output.success());
    assert_eq!(output.code, Some(1));

    let err = router.run_checked("show nonsense").await.unwrap_err();
    assert!(matches!(err, ToolError::CommandFailed { .. }));
    assert!(err.to_string().contains("syntax error"));
}

#[tokio::test]
async fn timeout_is_enforced() {
    let dir = tempfile::tempdir().expect("temp dir");
    let birdc = script(dir.path(), "birdc", "sleep 5");
    let router = local_router(&birdc, 1);

    let started = std::time::Instant::now();
    let err = router.run("show status").await.unwrap_err();
    assert!(matches!(err, ToolError::Timeout { .. }));
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[tokio::test]
async fn missing_birdc_is_a_spawn_error() {
    let router = local_router("/nonexistent/birdc", 5);
    let err = router.run("show status").await.unwrap_err();
    assert!(matches!(err, ToolError::Spawn { .. }));
    assert!(err.to_string().contains("/nonexistent/birdc"));
}

#[tokio::test]
async fn local_birdc_is_started_without_socket_argument_when_unset() {
    let dir = tempfile::tempdir().expect("temp dir");
    let birdc = script(dir.path(), "birdc", r#"printf '%s\n' "$@""#);
    let mut router = local_router(&birdc, 5);
    router.socket = None;

    let output = router.run("show route").await.expect("run");
    assert_eq!(
        output.stdout.lines().collect::<Vec<_>>(),
        vec!["show route"]
    );
}

#[tokio::test]
async fn ssh_command_is_quoted_for_the_remote_shell() {
    let dir = tempfile::tempdir().expect("temp dir");
    let ssh = script(dir.path(), "ssh", r#"printf '%s\n' "$@""#);
    let router = Router {
        name: "edge".to_string(),
        transport: TransportKind::Ssh,
        host: Some("router.example.net".to_string()),
        user: Some("bird".to_string()),
        port: Some(2222),
        birdc: "/usr/local/sbin/birdc".to_string(),
        socket: Some("/run/bird/bird.ctl".to_string()),
        config_file: "/etc/bird/bird.conf".to_string(),
        read_only: true,
        timeout: Duration::from_secs(5),
        ssh_bin: ssh,
    };

    let output = router
        .run("show route where net ~ [ 192.0.2.0/24+ ]")
        .await
        .expect("run");

    let args: Vec<&str> = output.stdout.lines().collect();
    assert_eq!(args[0], "-o");
    assert_eq!(args[1], "BatchMode=yes");
    assert_eq!(args[3], "ConnectTimeout=10");
    assert_eq!(args[4], "-p");
    assert_eq!(args[5], "2222");
    assert_eq!(args[6], "bird@router.example.net");
    assert_eq!(args[7], "--");
    assert_eq!(
        args[8],
        "'/usr/local/sbin/birdc' -s '/run/bird/bird.ctl' 'show route where net ~ [ 192.0.2.0/24+ ]'"
    );
}

#[tokio::test]
async fn read_file_reads_locally() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("bird.conf");
    std::fs::write(&path, "router id 10.0.0.1;\n").expect("write");
    let router = local_router("/bin/false", 5);

    let content = router
        .read_file(&path.to_string_lossy())
        .await
        .expect("read");
    assert_eq!(content, "router id 10.0.0.1;\n");
}

#[tokio::test]
async fn read_file_requires_absolute_path() {
    let router = local_router("/bin/false", 5);
    let err = router.read_file("bird.conf").await.unwrap_err();
    assert!(err.to_string().contains("absolute"));
}

#[test]
fn shell_quote_escapes_single_quotes() {
    assert_eq!(shell_quote("show route"), "'show route'");
    assert_eq!(shell_quote("it's"), "'it'\\''s'");
}
