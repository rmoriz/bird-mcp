//! End-to-end test of the compiled binary over the MCP stdio transport, with a
//! fake birdc that replays recorded BIRD output.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

const PROTOCOL_VERSION: &str = "2026-07-28";

struct Server {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Server {
    async fn start(birdc: &Path, home: &Path) -> Server {
        let routers = json!([{
            "name": "lab",
            "transport": "local",
            "birdc": birdc.to_string_lossy(),
            "read_only": true,
        }]);

        let mut child = Command::new(env!("CARGO_BIN_EXE_bird-mcp"))
            .env("BIRD_MCP_ROUTERS", routers.to_string())
            .env("HOME", home)
            .env_remove("BIRD_MCP_CONFIG")
            .env("RUST_LOG", "bird_mcp=debug")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn bird-mcp");

        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        Server {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
        }
    }

    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await.expect("write");
        self.stdin.flush().await.expect("flush");
    }

    async fn response(&mut self, id: u64) -> Value {
        loop {
            let line = self
                .lines
                .next_line()
                .await
                .expect("read stdout")
                .expect("server closed stdout");
            let value: Value = serde_json::from_str(&line).expect("JSON-RPC line");
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value;
            }
        }
    }

    async fn initialize(&mut self) {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "bird-mcp-test", "version": "0.1" }
            }
        }))
        .await;
        let response = self.response(1).await;
        assert_eq!(response["result"]["serverInfo"]["name"], "bird-mcp");

        self.send(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .await;
    }

    async fn call(&mut self, id: u64, tool: &str, arguments: Value) -> Value {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": tool, "arguments": arguments }
        }))
        .await;
        self.response(id).await
    }

    async fn shutdown(mut self) {
        drop(self.stdin);
        let _ = tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await;
        let _ = self.child.kill().await;
    }
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn fake_birdc(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("birdc");
    let body = format!(
        r#"#!/bin/sh
case "$1" in
  "show status") cat '{}' ;;
  "show route") cat '{}' ;;
  "show route all") cat '{}' ;;
  "show protocols") cat '{}' ;;
  *) echo "unexpected command: $*" >&2; exit 1 ;;
esac
"#,
        fixture("status.txt"),
        fixture("route.txt"),
        fixture("route-all.txt"),
        fixture("protocols.txt"),
    );
    std::fs::write(&path, body).expect("write fake birdc");
    let mut permissions = std::fs::metadata(&path).expect("stat").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("chmod");
    path
}

async fn run_test() {
    let dir = tempfile::tempdir().expect("temp dir");
    let birdc = fake_birdc(dir.path());
    let mut server = Server::start(&birdc, dir.path()).await;
    server.initialize().await;

    // Tool discovery.
    server
        .send(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .await;
    let list = server.response(2).await;
    let tools: Vec<&str> = list["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for expected in [
        "bird_routers",
        "bird_status",
        "bird_protocols",
        "bird_routes",
        "bird_interfaces",
        "bird_show",
        "bird_config_read",
        "bird_configure",
        "bird_protocol_control",
        "bird_docs_search",
        "bird_docs_section",
        "bird_docs_read",
    ] {
        assert!(tools.contains(&expected), "missing tool {expected}");
    }

    // Parsed status.
    let status = server.call(3, "bird_status", json!({})).await;
    let text = status["result"]["content"][0]["text"]
        .as_str()
        .expect("status text");
    let body: Value = serde_json::from_str(text).expect("status JSON");
    assert_eq!(body["router"], "lab");
    assert_eq!(body["status"]["version"], "3.4.0");
    assert_eq!(body["status"]["router_id"], "10.255.0.1");

    // Parsed routes.
    let routes = server.call(4, "bird_routes", json!({})).await;
    let text = routes["result"]["content"][0]["text"]
        .as_str()
        .expect("routes text");
    let body: Value = serde_json::from_str(text).expect("routes JSON");
    assert_eq!(body["routes"].as_array().expect("routes").len(), 5);
    assert_eq!(body["truncated"], false);

    // Read-only refusals happen before birdc is invoked.
    let control = server
        .call(
            5,
            "bird_protocol_control",
            json!({ "action": "restart", "target": "peer2" }),
        )
        .await;
    assert!(
        control.to_string().contains("read-only"),
        "unexpected response: {control}"
    );

    // bird_show accepts only read-only commands.
    let refused = server
        .call(6, "bird_show", json!({ "command": "configure" }))
        .await;
    assert!(
        refused
            .to_string()
            .contains("only accepts read-only commands"),
        "unexpected response: {refused}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn stdio_end_to_end() {
    tokio::time::timeout(Duration::from_secs(60), run_test())
        .await
        .expect("test timed out");
}
