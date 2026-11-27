//! MCP tool surface for BIRD.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::{Config, Router};
use crate::docs::Docs;
use crate::error::{ToolError, ToolResult};
use crate::parse;

#[derive(Clone)]
pub struct Bird {
    state: Arc<State>,
    #[expect(dead_code, reason = "the tool_handler macro reads this field")]
    tool_router: ToolRouter<Bird>,
}

struct State {
    config: Config,
}

impl Bird {
    pub fn new(config: Config) -> Bird {
        Bird {
            state: Arc::new(State { config }),
            tool_router: Bird::tool_router(),
        }
    }

    fn router(&self, name: Option<&str>) -> ToolResult<&Router> {
        self.state.config.router(name)
    }

    fn docs(&self) -> ToolResult<Docs> {
        Docs::locate(self.state.config.docs_dir.clone())
    }

    fn require_write(router: &Router, op: &str) -> ToolResult<()> {
        if router.read_only {
            return Err(ToolError::ReadOnly {
                router: router.name.clone(),
                op: op.to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct RoutersArgs {
    /// Run `show status` on every router to check reachability (default: false).
    #[serde(default)]
    pub probe: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusArgs {
    /// Router name from the configuration. Omit when only one router is configured.
    #[serde(default)]
    pub router: Option<String>,
    /// Include the unparsed birdc output (default: false).
    #[serde(default)]
    pub raw: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProtocolsArgs {
    /// Router name from the configuration. Omit when only one router is configured.
    #[serde(default)]
    pub router: Option<String>,
    /// Protocol name or pattern (for example "bgp*"). Omit for all protocols.
    #[serde(default)]
    pub name: Option<String>,
    /// Include per-protocol details (`show protocols all`), including BGP sessions (default: false).
    #[serde(default)]
    pub details: Option<bool>,
    /// Include the unparsed birdc output (default: false).
    #[serde(default)]
    pub raw: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    /// Only this exact prefix.
    #[default]
    Exact,
    /// The route used to forward to this address or prefix (longest match).
    For,
    /// Every prefix covered by this prefix.
    In,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RoutesArgs {
    /// Router name from the configuration. Omit when only one router is configured.
    #[serde(default)]
    pub router: Option<String>,
    /// Prefix or IP address, for example 192.0.2.0/24 or 2001:db8::1.
    #[serde(default)]
    pub prefix: Option<String>,
    /// How to interpret `prefix`: exact (default), for (forwarding lookup), in (covered prefixes).
    #[serde(default)]
    pub match_mode: Option<MatchMode>,
    /// Routing table, for example master4, master6 or all. Default: BIRD's default tables.
    #[serde(default)]
    pub table: Option<String>,
    /// Only routes from this protocol (name or pattern).
    #[serde(default)]
    pub protocol: Option<String>,
    /// Include route attributes (`show route all`).
    #[serde(default)]
    pub verbose: Option<bool>,
    /// Only the selected route per network.
    #[serde(default)]
    pub primary: Option<bool>,
    /// Return only route statistics.
    #[serde(default)]
    pub count_only: Option<bool>,
    /// Maximum routes to return (default 200, max 5000).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Include the unparsed birdc output (default: false).
    #[serde(default)]
    pub raw: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InterfacesArgs {
    /// Router name from the configuration. Omit when only one router is configured.
    #[serde(default)]
    pub router: Option<String>,
    /// One line per interface instead of full details (default: false).
    #[serde(default)]
    pub summary: Option<bool>,
    /// Include the unparsed birdc output (default: false).
    #[serde(default)]
    pub raw: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowArgs {
    /// Router name from the configuration. Omit when only one router is configured.
    #[serde(default)]
    pub router: Option<String>,
    /// A read-only BIRD command, for example `show ospf neighbors` or `show bfd sessions`.
    pub command: String,
    /// Maximum output lines (default 400, max 2000).
    #[serde(default)]
    pub max_lines: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigReadArgs {
    /// Router name from the configuration. Omit when only one router is configured.
    #[serde(default)]
    pub router: Option<String>,
    /// First line to return (1-based, default 1).
    #[serde(default)]
    pub start_line: Option<u32>,
    /// Maximum lines to return (default 400, max 2000).
    #[serde(default)]
    pub max_lines: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigureMode {
    /// Parse and validate the configuration without applying it. Safe in read-only mode.
    Check,
    /// Apply the configuration. Runs `configure check` first when a file is given.
    Apply,
    /// Revert to the previous configuration.
    Undo,
    /// Confirm the configuration and cancel the undo timer.
    Confirm,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigureArgs {
    /// Router name from the configuration. Omit when only one router is configured.
    #[serde(default)]
    pub router: Option<String>,
    /// check, apply, undo or confirm.
    pub mode: ConfigureMode,
    /// Configuration file on the router. Default: the file BIRD already uses.
    #[serde(default)]
    pub file: Option<String>,
    /// Apply without restarting protocols whose filters changed (default: false).
    #[serde(default)]
    pub soft: Option<bool>,
    /// Seconds until BIRD reverts automatically unless confirmed (`configure timeout`).
    #[serde(default)]
    pub timeout_secs: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolAction {
    Enable,
    Disable,
    Restart,
    Reload,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProtocolControlArgs {
    /// Router name from the configuration. Omit when only one router is configured.
    #[serde(default)]
    pub router: Option<String>,
    /// enable, disable, restart or reload.
    pub action: ProtocolAction,
    /// Protocol name or quoted pattern, for example bgp1 or "bgp*".
    pub target: String,
    /// Reload direction: in, out, or both when omitted.
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocsSearchArgs {
    /// Words to look for; all of them must appear on the same line.
    pub query: String,
    /// Maximum hits (default 8, max 40).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocsSectionArgs {
    /// Documentation label, for example cli-show-route, cli-configure-check or protocols-bgp.
    pub label: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocsReadArgs {
    /// File relative to the docs directory, for example bird.sgml or bird.conf.example.
    pub file: String,
    /// First line (1-based, default 1).
    #[serde(default)]
    pub start_line: Option<u32>,
    /// Maximum lines (default 200, max 500).
    #[serde(default)]
    pub max_lines: Option<u32>,
}

#[tool_router]
impl Bird {
    #[tool(
        description = "List the BIRD routers this server can reach, with transport, write access and \
                       (optionally) a reachability probe."
    )]
    async fn bird_routers(
        &self,
        Parameters(args): Parameters<RoutersArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let probe = args.probe.unwrap_or(false);
        let mut routers = Vec::new();

        for router in &self.state.config.routers {
            let mut entry = json!({
                "name": router.name,
                "transport": router.transport,
                "target": router.target(),
                "read_only": router.read_only,
                "config_file": router.config_file,
                "timeout_secs": router.timeout.as_secs(),
            });
            if probe {
                match router.run_checked("show status").await {
                    Ok(output) => {
                        let status = parse::parse_status(&output.stdout);
                        entry["status"] = json!(status);
                    }
                    Err(err) => entry["error"] = json!(err.to_string()),
                }
            }
            routers.push(entry);
        }

        respond(json!({ "routers": routers }))
    }

    #[tool(
        description = "Show BIRD router status: version, router ID, hostname, uptime (last reboot), \
                       last reconfiguration and daemon state."
    )]
    async fn bird_status(
        &self,
        Parameters(args): Parameters<StatusArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let router = self.router(args.router.as_deref())?;
        let output = router.run_checked("show status").await?;
        let status = parse::parse_status(&output.stdout);

        let mut body = json!({ "router": router.name, "status": status });
        if args.raw.unwrap_or(false) {
            body["raw"] = json!(parse::strip_banner(&output.stdout));
        }
        respond(body)
    }

    #[tool(
        description = "Show BIRD protocol instances (device, kernel, static, BGP, OSPF, ...). With \
                       details=true this includes BGP session state, timers, channel filters and \
                       route counters. Filter with a protocol name or pattern."
    )]
    async fn bird_protocols(
        &self,
        Parameters(args): Parameters<ProtocolsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let router = self.router(args.router.as_deref())?;
        let mut command = String::from("show protocols");
        if args.details.unwrap_or(false) {
            command.push_str(" all");
        }
        if let Some(name) = &args.name {
            validate_pattern(name, "name")?;
            command.push(' ');
            command.push_str(name);
        }

        let output = router.run_checked(&command).await?;
        let protocols = parse::parse_protocols(&output.stdout);

        let mut body = json!({
            "router": router.name,
            "command": command,
            "protocols": protocols,
        });
        if args.raw.unwrap_or(false) {
            body["raw"] = json!(parse::strip_banner(&output.stdout));
        }
        respond(body)
    }

    #[tool(
        description = "Show routes from BIRD's routing tables, parsed into prefix, destination type, \
                       protocol, preference, next hops and attributes. Use prefix with match_mode \
                       'for' for a forwarding lookup, or count_only for table statistics. Large \
                       tables are truncated to `limit` routes."
    )]
    async fn bird_routes(
        &self,
        Parameters(args): Parameters<RoutesArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let router = self.router(args.router.as_deref())?;
        let mut command = String::from("show route");

        if let Some(prefix) = &args.prefix {
            validate_prefix(prefix)?;
            match args.match_mode.unwrap_or_default() {
                MatchMode::Exact => {
                    command.push(' ');
                    command.push_str(prefix);
                }
                MatchMode::For => {
                    command.push_str(" for ");
                    command.push_str(prefix);
                }
                MatchMode::In => {
                    command.push_str(" in ");
                    command.push_str(prefix);
                }
            }
        }
        if let Some(table) = &args.table {
            validate_pattern(table, "table")?;
            command.push_str(" table ");
            command.push_str(table);
        }
        if args.verbose.unwrap_or(false) {
            command.push_str(" all");
        }
        if args.primary.unwrap_or(false) {
            command.push_str(" primary");
        }
        if let Some(protocol) = &args.protocol {
            validate_pattern(protocol, "protocol")?;
            command.push_str(" protocol ");
            command.push_str(protocol);
        }
        if args.count_only.unwrap_or(false) {
            command.push_str(" count");
        }

        let output = router.run_checked(&command).await?;
        let parsed = parse::parse_routes(&output.stdout);
        let limit = args.limit.unwrap_or(200).clamp(1, 5000) as usize;
        let parsed_count = parsed.routes.len();
        let routes: Vec<_> = parsed.routes.into_iter().take(limit).collect();

        let mut body = json!({
            "router": router.name,
            "command": command,
            "routes": routes,
            "returned": routes.len(),
            "parsed": parsed_count,
            "truncated": parsed_count > routes.len(),
            "counts": parsed.counts,
        });
        if !parsed.warnings.is_empty() {
            body["warnings"] = json!(parsed.warnings);
        }
        if args.raw.unwrap_or(false) {
            body["raw"] = json!(parse::strip_banner(&output.stdout));
        }
        respond(body)
    }

    #[tool(
        description = "Show BIRD interfaces with state, MTU, flags and addresses. summary=true gives one \
                       line per interface."
    )]
    async fn bird_interfaces(
        &self,
        Parameters(args): Parameters<InterfacesArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let router = self.router(args.router.as_deref())?;
        let summary = args.summary.unwrap_or(false);
        let command = if summary {
            "show interfaces summary"
        } else {
            "show interfaces"
        };
        let output = router.run_checked(command).await?;

        let mut body = if summary {
            json!({
                "router": router.name,
                "summary": true,
                "interfaces": parse::parse_interface_summary(&output.stdout),
            })
        } else {
            json!({
                "router": router.name,
                "summary": false,
                "interfaces": parse::parse_interfaces(&output.stdout),
            })
        };
        if args.raw.unwrap_or(false) {
            body["raw"] = json!(parse::strip_banner(&output.stdout));
        }
        respond(body)
    }

    #[tool(
        description = "Run a read-only BIRD CLI command that has no dedicated tool, for example \
                       `show bfd sessions`, `show ospf neighbors`, `show symbols` or `show memory`. \
                       Only commands starting with `show` are accepted."
    )]
    async fn bird_show(
        &self,
        Parameters(args): Parameters<ShowArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let router = self.router(args.router.as_deref())?;
        let command = validate_show(&args.command)?;
        let output = router.run_checked(&command).await?;
        let text = truncate_lines(parse::strip_banner(&output.stdout), args.max_lines, 400);

        let mut body = json!({
            "router": router.name,
            "command": command,
            "output": text.text,
        });
        if text.truncated {
            body["truncated"] = json!(true);
        }
        respond(body)
    }

    #[tool(
        description = "Read the BIRD configuration file from the router. Works in read-only mode; use \
                       bird_configure with mode=check to validate a change before applying it."
    )]
    async fn bird_config_read(
        &self,
        Parameters(args): Parameters<ConfigReadArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let router = self.router(args.router.as_deref())?;
        let text = router.read_file(&router.config_file).await?;
        let lines: Vec<&str> = text.lines().collect();
        let start = args.start_line.unwrap_or(1).max(1) as usize;
        let max = args.max_lines.unwrap_or(400).clamp(1, 2000) as usize;
        let start_index = (start - 1).min(lines.len());
        let end_index = (start_index + max).min(lines.len());

        respond(json!({
            "router": router.name,
            "path": router.config_file,
            "start_line": start_index + 1,
            "end_line": end_index,
            "total_lines": lines.len(),
            "text": lines[start_index..end_index].join("\n"),
        }))
    }

    #[tool(
        description = "Check, apply, undo or confirm a BIRD configuration. mode=check only parses the \
                       file and is allowed on read-only routers; apply/undo/confirm need read_only = \
                       false. apply runs `configure check` first when `file` is given."
    )]
    async fn bird_configure(
        &self,
        Parameters(args): Parameters<ConfigureArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let router = self.router(args.router.as_deref())?;
        let file = args.file.as_deref().map(validate_config_path).transpose()?;

        let quoted_file = file.as_deref().map(|file| format!("\"{file}\""));

        match args.mode {
            ConfigureMode::Check => {
                let mut command = String::from("configure check");
                if let Some(file) = &quoted_file {
                    command.push(' ');
                    command.push_str(file);
                }
                let output = router.run_checked(&command).await?;
                let check = parse::parse_config_check(&output.stdout);
                respond(json!({
                    "router": router.name,
                    "mode": "check",
                    "command": command,
                    "ok": check.ok,
                    "check": check,
                }))
            }
            ConfigureMode::Apply => {
                Self::require_write(router, "configure apply")?;

                let mut checked = None;
                if let Some(file) = &quoted_file {
                    let check_command = format!("configure check {file}");
                    let output = router.run_checked(&check_command).await?;
                    let check = parse::parse_config_check(&output.stdout);
                    if !check.ok {
                        return Err(ToolError::BadArgs(format!(
                            "configuration check failed, nothing applied: {}",
                            check.message
                        ))
                        .into());
                    }
                    checked = Some(json!({
                        "command": check_command,
                        "ok": check.ok,
                        "message": check.message,
                    }));
                }

                let mut command = String::from("configure");
                if args.soft.unwrap_or(false) {
                    command.push_str(" soft");
                }
                if let Some(file) = &quoted_file {
                    command.push(' ');
                    command.push_str(file);
                }
                if let Some(secs) = args.timeout_secs {
                    command.push_str(&format!(" timeout {}", secs.clamp(1, 86_400)));
                }

                let output = router.run_checked(&command).await?;
                let mut body = json!({
                    "router": router.name,
                    "mode": "apply",
                    "command": command,
                    "ok": true,
                    "output": parse::strip_banner(&output.stdout).trim(),
                });
                if let Some(checked) = checked {
                    body["check"] = checked;
                }
                respond(body)
            }
            ConfigureMode::Undo => {
                Self::require_write(router, "configure undo")?;
                let output = router.run_checked("configure undo").await?;
                respond(json!({
                    "router": router.name,
                    "mode": "undo",
                    "command": "configure undo",
                    "ok": true,
                    "output": parse::strip_banner(&output.stdout).trim(),
                }))
            }
            ConfigureMode::Confirm => {
                Self::require_write(router, "configure confirm")?;
                let output = router.run_checked("configure confirm").await?;
                respond(json!({
                    "router": router.name,
                    "mode": "confirm",
                    "command": "configure confirm",
                    "ok": true,
                    "output": parse::strip_banner(&output.stdout).trim(),
                }))
            }
        }
    }

    #[tool(
        description = "Enable, disable, restart or reload BIRD protocol instances. Needs read_only = \
                       false on the router. Target is a protocol name or a quoted pattern such as \
                       \"bgp*\"."
    )]
    async fn bird_protocol_control(
        &self,
        Parameters(args): Parameters<ProtocolControlArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let router = self.router(args.router.as_deref())?;
        Self::require_write(router, "protocol control")?;
        validate_pattern(&args.target, "target")?;

        let mut command = match args.action {
            ProtocolAction::Enable => "enable".to_string(),
            ProtocolAction::Disable => "disable".to_string(),
            ProtocolAction::Restart => "restart".to_string(),
            ProtocolAction::Reload => {
                let direction = match args.direction.as_deref() {
                    None => "",
                    Some(value) if value.eq_ignore_ascii_case("in") => " in",
                    Some(value) if value.eq_ignore_ascii_case("out") => " out",
                    Some(other) => {
                        return Err(ToolError::BadArgs(format!(
                            "direction must be in or out, not '{other}'"
                        ))
                        .into());
                    }
                };
                format!("reload{direction}")
            }
        };
        command.push(' ');
        command.push_str(&args.target);

        let output = router.run_checked(&command).await?;
        respond(json!({
            "router": router.name,
            "command": command,
            "ok": true,
            "output": parse::strip_banner(&output.stdout).trim(),
        }))
    }

    #[tool(
        description = "Search the BIRD documentation bundled with the source checkout (bird.sgml, \
                       bird.conf.example, migration notes). Returns file:line hits."
    )]
    async fn bird_docs_search(
        &self,
        Parameters(args): Parameters<DocsSearchArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let docs = self.docs()?;
        let hits = docs.search(&args.query, args.limit.unwrap_or(8) as usize)?;
        respond(json!({
            "query": args.query,
            "docs_root": docs.root(),
            "hits": hits,
        }))
    }

    #[tool(
        description = "Read one documented BIRD feature by its label, for example cli-show-route, \
                       cli-configure-check, protocols-bgp or filters. Use bird_docs_search if the \
                       label is unknown."
    )]
    async fn bird_docs_section(
        &self,
        Parameters(args): Parameters<DocsSectionArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let docs = self.docs()?;
        let section = docs.section(&args.label)?;
        respond(json!(section))
    }

    #[tool(
        description = "Read a line range from one BIRD documentation file, for example bird.sgml or \
                       bird.conf.example."
    )]
    async fn bird_docs_read(
        &self,
        Parameters(args): Parameters<DocsReadArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let docs = self.docs()?;
        let chunk = docs.read(
            &args.file,
            args.start_line.unwrap_or(1) as usize,
            args.max_lines.unwrap_or(200) as usize,
        )?;
        respond(json!(chunk))
    }
}

#[tool_handler]
impl ServerHandler for Bird {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            rmcp::model::Implementation::new("bird-mcp", env!("CARGO_PKG_VERSION")),
        )
    }
}

fn respond(value: Value) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string_pretty(&value).map_err(|err| {
        ErrorData::internal_error(format!("cannot serialize tool response: {err}"), None)
    })?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

struct Truncated {
    text: String,
    truncated: bool,
}

fn truncate_lines(text: &str, requested: Option<u32>, default: u32) -> Truncated {
    let max = requested.unwrap_or(default).clamp(1, 2000) as usize;
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max {
        return Truncated {
            text: text.trim_end().to_string(),
            truncated: false,
        };
    }
    let mut text = lines[..max].join("\n");
    text.push_str(&format!(
        "\n... truncated: {} of {} lines shown; narrow the command or read the rest with \
         bird_show max_lines.",
        max,
        lines.len()
    ));
    Truncated {
        text,
        truncated: true,
    }
}

fn validate_pattern(value: &str, field: &str) -> ToolResult<()> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ToolError::BadArgs(format!("{field} is empty")));
    }

    // A pattern may be quoted by the caller; BIRD expects the quotes.
    let (inner, quoted) = match (value.strip_prefix('"'), value.strip_suffix('"')) {
        (Some(inner), Some(_)) if value.len() >= 2 => (inner, true),
        _ => (value, false),
    };

    let allowed = inner
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "_.*-?".contains(character));
    if !allowed || inner.is_empty() {
        return Err(ToolError::BadArgs(format!(
            "{field} contains characters BIRD does not allow in a name or pattern: {value}"
        )));
    }
    if quoted && inner.contains('"') {
        return Err(ToolError::BadArgs(format!("{field} has unbalanced quotes")));
    }
    Ok(())
}

fn validate_prefix(value: &str) -> ToolResult<()> {
    let value = value.trim();
    let allowed = value
        .chars()
        .all(|character| character.is_ascii_hexdigit() || ":/.".contains(character));
    let slashes = value.matches('/').count();
    if !allowed || value.is_empty() || slashes > 1 {
        return Err(ToolError::BadArgs(format!(
            "not an IP address or prefix: {value}"
        )));
    }
    Ok(())
}

fn validate_config_path(value: &str) -> ToolResult<String> {
    let value = value.trim();
    if !value.starts_with('/')
        || value.len() > 512
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_./+~-".contains(character))
    {
        return Err(ToolError::BadArgs(format!(
            "config file must be an absolute path with simple characters: {value}"
        )));
    }
    Ok(value.to_string())
}

fn validate_show(command: &str) -> ToolResult<String> {
    let command = command.trim();
    let first = command.split_whitespace().next().unwrap_or("");
    if first != "show" {
        return Err(ToolError::BadArgs(
            "bird_show only accepts read-only commands starting with `show`; use \
             bird_protocol_control or bird_configure for changes"
                .to_string(),
        ));
    }
    if command.len() > 1000 {
        return Err(ToolError::BadArgs("command is too long".to_string()));
    }
    if command
        .chars()
        .any(|character| matches!(character, '\n' | '\r' | ';' | '`' | '\\' | '$'))
    {
        return Err(ToolError::BadArgs(
            "command contains characters that are not allowed".to_string(),
        ));
    }
    Ok(command.to_string())
}
