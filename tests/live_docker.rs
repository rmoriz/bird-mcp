#![cfg(unix)]

//! Live end-to-end test against a real BIRD 3.4 instance.
//!
//! The lab used for this repository runs BIRD 3.4.0 in a container named
//! `bird-r1` with a control socket at /run/bird/bird.ctl. Start one with:
//!
//! ```text
//! docker run -d --name bird-r1 --network birdnet --ip 172.30.0.11 \
//!   -v "$PWD/r1.conf:/etc/bird/bird.conf:ro" bird:3.4.0
//! ```
//!
//! Run with: `cargo test --test live_docker -- --ignored`

use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

struct Server {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Server {
    async fn start(birdc: &str, _home: &std::path::Path) -> Server {
        let routes_file = format!(
            "{}/tests/docker/state/bird-mcp.conf",
            env!("CARGO_MANIFEST_DIR")
        );
        let routers = json!([
            { "name": "r1", "birdc": birdc, "read_only": true },
            {
                "name": "rw",
                "birdc": birdc,
                "read_only": false,
                "routes_file": routes_file
            }
        ]);
        let docs = format!("{}/bird/doc", env!("CARGO_MANIFEST_DIR"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_bird-mcp"))
            .env("BIRD_MCP_ROUTERS", routers.to_string())
            .env("BIRD_MCP_DOCS_DIR", docs)
            .env_remove("BIRD_MCP_CONFIG")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
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
            let line = self.lines.next_line().await.expect("read").expect("closed");
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
                "protocolVersion": "2026-07-28",
                "capabilities": {},
                "clientInfo": { "name": "bird-mcp-live", "version": "0.1" }
            }
        }))
        .await;
        self.response(1).await;
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
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
}

fn body(response: &Value) -> Value {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no content: {response}"));
    serde_json::from_str(text).expect("tool response JSON")
}

async fn run() {
    let dir = tempfile::tempdir().expect("temp dir");
    let birdc = dir.path().join("birdc");
    std::fs::write(
        &birdc,
        "#!/bin/sh\nexec docker exec bird-r1 birdc -s /run/bird/bird.ctl \"$@\"\n",
    )
    .expect("write wrapper");
    let mut permissions = std::fs::metadata(&birdc).expect("stat").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&birdc, permissions).expect("chmod");

    let mut server = Server::start(&birdc.to_string_lossy(), dir.path()).await;
    server.initialize().await;

    let status = body(&server.call(2, "bird_status", json!({})).await);
    assert_eq!(status["status"]["version"], "3.4.0");
    assert!(status["status"]["router_id"].is_string());

    let interfaces = body(&server.call(3, "bird_interfaces", json!({})).await);
    let names: Vec<&str> = interfaces["interfaces"]
        .as_array()
        .expect("interfaces")
        .iter()
        .filter_map(|interface| interface["name"].as_str())
        .collect();
    assert!(names.contains(&"eth0"), "interfaces: {names:?}");

    let routes = body(
        &server
            .call(4, "bird_routes", json!({ "count_only": true }))
            .await,
    );
    assert!(
        routes["counts"].as_array().expect("counts").len() >= 2,
        "counts: {}",
        routes["counts"]
    );

    let protocols = body(&server.call(5, "bird_protocols", json!({})).await);
    assert!(protocols["protocols"].as_array().expect("protocols").len() >= 2);

    let check = body(
        &server
            .call(6, "bird_configure", json!({ "mode": "check" }))
            .await,
    );
    assert_eq!(check["ok"], true, "config check: {check}");

    // BGP needs a moment to come up on a fresh lab, so poll for Established.
    let mut established = false;
    let mut details = Value::Null;
    for attempt in 0..20u64 {
        details = body(
            &server
                .call(
                    100 + attempt,
                    "bird_protocols",
                    json!({ "name": "peer2", "details": true }),
                )
                .await,
        );
        established = details["protocols"][0]["attrs"]
            .as_array()
            .is_some_and(|attrs| {
                attrs
                    .iter()
                    .any(|attr| attr["key"] == "BGP state" && attr["value"] == "Established")
            });
        if established {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(established, "bgp did not come up: {details}");

    let memory = body(
        &server
            .call(8, "bird_show", json!({ "command": "show memory" }))
            .await,
    );
    assert!(
        memory["output"]
            .as_str()
            .is_some_and(|output| output.contains("BIRD memory usage"))
    );

    let hits = body(
        &server
            .call(9, "bird_docs_search", json!({ "query": "hold timer" }))
            .await,
    );
    assert!(
        !hits["hits"].as_array().expect("hits").is_empty(),
        "docs hits: {hits}"
    );

    let section = body(
        &server
            .call(
                10,
                "bird_docs_section",
                json!({ "label": "cli-show-route" }),
            )
            .await,
    );
    assert!(
        section["text"]
            .as_str()
            .is_some_and(|text| text.contains("show route"))
    );

    // Write path: restart a protocol and re-read the running config.
    let restarted = body(
        &server
            .call(
                11,
                "bird_protocol_control",
                json!({ "router": "rw", "action": "restart", "target": "peer2" }),
            )
            .await,
    );
    assert_eq!(restarted["ok"], true, "restart: {restarted}");

    let applied = body(
        &server
            .call(
                12,
                "bird_configure",
                json!({ "router": "rw", "mode": "apply" }),
            )
            .await,
    );
    assert_eq!(applied["ok"], true, "apply: {applied}");
    assert!(
        applied["output"]
            .as_str()
            .is_some_and(|output| output.contains("Reconfigured")),
        "apply output: {applied}"
    );

    // A blackhole route added through the managed include file must show up in
    // BIRD and disappear again after removal.
    let added = body(
        &server
            .call(
                13,
                "bird_routes_edit",
                json!({
                    "router": "rw",
                    "action": "add",
                    "prefix": "203.99.0.0/24"
                }),
            )
            .await,
    );
    assert_eq!(added["changed"], true, "route add: {added}");
    assert_eq!(added["verification"]["ok"], true, "route add: {added}");

    let installed = body(
        &server
            .call(14, "bird_routes", json!({ "prefix": "203.99.0.0/24" }))
            .await,
    );
    assert!(
        installed["routes"]
            .as_array()
            .expect("routes")
            .iter()
            .any(|route| route["protocol"] == "bird_mcp_static4"),
        "route not installed: {installed}"
    );

    let removed = body(
        &server
            .call(
                15,
                "bird_routes_edit",
                json!({
                    "router": "rw",
                    "action": "remove",
                    "prefix": "203.99.0.0/24"
                }),
            )
            .await,
    );
    assert_eq!(removed["removed"], 1, "route remove: {removed}");

    let gone = body(
        &server
            .call(16, "bird_routes", json!({ "prefix": "203.99.0.0/24" }))
            .await,
    );
    assert!(
        !gone["routes"]
            .as_array()
            .expect("routes")
            .iter()
            .any(|route| route["protocol"] == "bird_mcp_static4"),
        "route still installed: {gone}"
    );

    drop(server.stdin);
    let _ = tokio::time::timeout(Duration::from_secs(5), server.child.wait()).await;
    let _ = server.child.kill().await;
}

#[tokio::test]
#[ignore = "needs the docker lab with a running BIRD 3.4 (bird-r1)"]
async fn live_bird_3_4() {
    tokio::time::timeout(Duration::from_secs(120), run())
        .await
        .expect("live test timed out");
}
