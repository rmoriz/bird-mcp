//! Server configuration: which BIRD instances to talk to and how.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::error::{ToolError, ToolResult};

const DEFAULT_TIMEOUT_SECS: u64 = 20;
const DEFAULT_CONFIG_FILE: &str = "/etc/bird/bird.conf";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    /// Run `birdc` on this machine.
    Local,
    /// Run `birdc` on a remote machine through ssh.
    Ssh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Router {
    pub name: String,
    pub transport: TransportKind,
    pub host: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// Path or name of the birdc binary on the machine that runs BIRD.
    pub birdc: String,
    /// Control socket; `None` lets birdc use its compiled-in default.
    pub socket: Option<String>,
    /// Config file used by `bird_config_read` and as the default target of `bird_configure`.
    pub config_file: String,
    /// Read-only unless the operator explicitly opted in.
    pub read_only: bool,
    pub timeout: Duration,
    /// ssh binary used for the ssh transport.
    pub ssh_bin: String,
}

impl Router {
    pub fn target(&self) -> String {
        match self.transport {
            TransportKind::Local => "local".to_string(),
            TransportKind::Ssh => match &self.user {
                Some(user) => format!("{user}@{}", self.host.as_deref().unwrap_or("?")),
                None => self.host.clone().unwrap_or_else(|| "?".to_string()),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub routers: Vec<Router>,
    pub docs_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> ToolResult<Config> {
        let pairs: Vec<(String, String)> = std::env::vars().collect();
        let borrowed: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        Config::from_pairs(&borrowed)
    }

    /// The only place defaults live, so tests exercise the real path.
    pub fn from_pairs(pairs: &[(&str, &str)]) -> ToolResult<Config> {
        let map: HashMap<&str, &str> = pairs.iter().copied().collect();
        let get = |key: &str| {
            map.get(key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
        };

        let default_timeout = match get("BIRD_MCP_TIMEOUT") {
            None => Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            Some(raw) => {
                let secs: u64 = raw.parse().map_err(|_| {
                    ToolError::Config(format!("BIRD_MCP_TIMEOUT is not a number: {raw}"))
                })?;
                Duration::from_secs(secs.clamp(1, 300))
            }
        };
        let ssh_bin = get("BIRD_MCP_SSH_BIN").unwrap_or("ssh").to_string();
        let mut file_docs_dir: Option<String> = None;

        let specs: Vec<RouterSpec> = if let Some(raw) = get("BIRD_MCP_ROUTERS") {
            parse_router_json(raw)?
        } else if let Some(path) = config_file_path(&map)? {
            let text = std::fs::read_to_string(&path).map_err(|err| {
                ToolError::Config(format!("cannot read {}: {err}", path.display()))
            })?;
            let file: FileConfig = toml::from_str(&text).map_err(|err| {
                ToolError::Config(format!("invalid config file {}: {err}", path.display()))
            })?;
            if file.routers.is_empty() {
                return Err(ToolError::Config(format!(
                    "config file {} has no [[routers]]",
                    path.display()
                )));
            }
            file_docs_dir = file.docs_dir;
            file.routers
        } else {
            vec![default_local_router(&map)]
        };

        let docs_dir = get("BIRD_MCP_DOCS_DIR")
            .map(PathBuf::from)
            .or_else(|| file_docs_dir.map(PathBuf::from));

        let mut routers = Vec::with_capacity(specs.len());
        for (index, spec) in specs.into_iter().enumerate() {
            routers.push(build_router(spec, index, default_timeout, &ssh_bin, &map)?);
        }

        for (index, router) in routers.iter().enumerate() {
            if routers[..index]
                .iter()
                .any(|other| other.name == router.name)
            {
                return Err(ToolError::Config(format!(
                    "duplicate router name '{}'",
                    router.name
                )));
            }
        }

        Ok(Config { routers, docs_dir })
    }

    pub fn router(&self, name: Option<&str>) -> ToolResult<&Router> {
        match name {
            Some(name) => self
                .routers
                .iter()
                .find(|router| router.name == name)
                .ok_or_else(|| ToolError::UnknownRouter {
                    name: name.to_string(),
                    known: self
                        .routers
                        .iter()
                        .map(|router| router.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                }),
            None => self.routers.first().ok_or_else(|| {
                ToolError::Config(
                    "no routers configured. Set BIRD_MCP_ROUTERS or BIRD_MCP_CONFIG.".to_string(),
                )
            }),
        }
    }

    pub fn summary(&self) -> String {
        let routers = self
            .routers
            .iter()
            .map(|router| {
                format!(
                    "{} ({}{})",
                    router.name,
                    router.target(),
                    if router.read_only { ", read-only" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{} router(s): {routers}", self.routers.len())
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RouterSpec {
    name: Option<String>,
    transport: Option<String>,
    host: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    birdc: Option<String>,
    socket: Option<String>,
    config_file: Option<String>,
    read_only: Option<bool>,
    timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    docs_dir: Option<String>,
    #[serde(default)]
    routers: Vec<RouterSpec>,
}

fn parse_router_json(raw: &str) -> ToolResult<Vec<RouterSpec>> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|err| ToolError::Config(format!("BIRD_MCP_ROUTERS is not valid JSON: {err}")))?;
    let specs = match value {
        serde_json::Value::Array(_) => serde_json::from_value(value),
        serde_json::Value::Object(ref object) if object.contains_key("routers") => {
            serde_json::from_value::<FileConfig>(value).map(|file| file.routers)
        }
        _ => {
            return Err(ToolError::Config(
                "BIRD_MCP_ROUTERS must be a JSON array of routers".to_string(),
            ));
        }
    };
    specs.map_err(|err| ToolError::Config(format!("BIRD_MCP_ROUTERS is invalid: {err}")))
}

fn config_file_path(map: &HashMap<&str, &str>) -> ToolResult<Option<PathBuf>> {
    if let Some(raw) = map
        .get("BIRD_MCP_CONFIG")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(raw);
        if !path.exists() {
            return Err(ToolError::Config(format!(
                "BIRD_MCP_CONFIG points to {}, which does not exist",
                path.display()
            )));
        }
        return Ok(Some(path));
    }

    let default = map
        .get("HOME")
        .map(|home| PathBuf::from(home).join(".config/bird-mcp/config.toml"));
    Ok(default.filter(|path| path.exists()))
}

fn default_local_router(map: &HashMap<&str, &str>) -> RouterSpec {
    let env = |key: &str| {
        map.get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    RouterSpec {
        name: Some("local".to_string()),
        transport: Some("local".to_string()),
        birdc: env("BIRD_MCP_BIRDC"),
        socket: env("BIRD_MCP_SOCKET"),
        config_file: env("BIRD_MCP_CONFIG_FILE"),
        ..RouterSpec::default()
    }
}

fn build_router(
    spec: RouterSpec,
    index: usize,
    default_timeout: Duration,
    ssh_bin: &str,
    map: &HashMap<&str, &str>,
) -> ToolResult<Router> {
    let transport = match (
        spec.transport.as_deref().map(str::to_ascii_lowercase),
        spec.host.as_deref(),
    ) {
        (Some(value), _) if value == "local" || value == "ssh" => {
            if value == "ssh" {
                TransportKind::Ssh
            } else {
                TransportKind::Local
            }
        }
        (None, Some(_)) => TransportKind::Ssh,
        (None, None) => TransportKind::Local,
        (Some(other), _) => {
            return Err(ToolError::Config(format!(
                "router #{}: unknown transport '{other}' (use local or ssh)",
                index + 1
            )));
        }
    };

    let name = match spec.name.map(|name| name.trim().to_string()) {
        Some(name) if !name.is_empty() => name,
        _ if transport == TransportKind::Local && index == 0 => "local".to_string(),
        _ => format!("router{}", index + 1),
    };

    if transport == TransportKind::Ssh && spec.host.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ToolError::Config(format!(
            "router '{name}': transport ssh needs a host"
        )));
    }

    let timeout = match spec.timeout_secs {
        Some(secs) => Duration::from_secs(secs.clamp(1, 300)),
        None => default_timeout,
    };

    Ok(Router {
        name,
        transport,
        host: spec.host.map(|host| host.trim().to_string()),
        user: spec.user.map(|user| user.trim().to_string()),
        port: spec.port,
        birdc: spec
            .birdc
            .or_else(|| {
                map.get("BIRD_MCP_BIRDC")
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "birdc".to_string()),
        socket: spec
            .socket
            .map(|socket| socket.trim().to_string())
            .filter(|socket| !socket.is_empty()),
        config_file: spec.config_file.unwrap_or_else(|| {
            map.get("BIRD_MCP_CONFIG_FILE")
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| DEFAULT_CONFIG_FILE.to_string())
        }),
        read_only: spec.read_only.unwrap_or(true),
        timeout,
        ssh_bin: ssh_bin.to_string(),
    })
}
