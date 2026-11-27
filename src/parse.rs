//! Parsers for the text output of `birdc` (BIRD 3.x).
//!
//! BIRD's CLI prints fixed-width tables and indented detail lines. The parsers
//! here are deliberately tolerant: a line that does not look like what we
//! expect is reported as a warning or kept as raw detail instead of failing the
//! whole call.

use serde::Serialize;

/// Removes the `BIRD <version> ready.` banner birdc prints before every command.
pub fn strip_banner(raw: &str) -> &str {
    if let Some((first, rest)) = raw.split_once('\n') {
        let first = first.trim_end_matches('\r');
        if first.starts_with("BIRD ") && first.ends_with(" ready.") {
            return rest;
        }
    }
    raw
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    pub version: String,
    pub router_id: Option<String>,
    pub hostname: Option<String>,
    pub server_time: Option<String>,
    pub last_reboot: Option<String>,
    pub last_reconfiguration: Option<String>,
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub other: Vec<String>,
}

pub fn parse_status(raw: &str) -> Status {
    let mut status = Status {
        version: String::new(),
        router_id: None,
        hostname: None,
        server_time: None,
        last_reboot: None,
        last_reconfiguration: None,
        state: None,
        other: Vec::new(),
    };

    for line in strip_banner(raw).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(version) = line.strip_prefix("BIRD ") {
            status.version = version.trim().to_string();
        } else if let Some(value) = line.strip_prefix("Router ID is ") {
            status.router_id = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Hostname is ") {
            status.hostname = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Current server time is ") {
            status.server_time = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Last reboot on ") {
            status.last_reboot = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Last reconfiguration on ") {
            status.last_reconfiguration = Some(value.trim().to_string());
        } else if line == "Daemon is up and running"
            || line == "Shutdown in progress"
            || line == "Reconfiguration in progress"
        {
            status.state = Some(line.to_string());
        } else {
            status.other.push(line.to_string());
        }
    }

    status
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Attr {
    pub key: String,
    pub value: String,
    pub indent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Protocol {
    pub name: String,
    pub proto: String,
    pub table: Option<String>,
    pub state: String,
    pub since: Option<String>,
    pub info: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attr>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

pub fn parse_protocols(raw: &str) -> Vec<Protocol> {
    let body = strip_banner(raw);
    let mut protocols: Vec<Protocol> = Vec::new();
    let mut in_table = false;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !in_table {
            if trimmed.starts_with("Name") && trimmed.contains("Proto") {
                in_table = true;
            }
            continue;
        }

        if line.chars().next().is_some_and(char::is_whitespace) {
            if let Some(current) = protocols.last_mut() {
                current.details.push(trimmed.to_string());
                if let Some(attr) = parse_attr(line) {
                    current.attrs.push(attr);
                }
            }
            continue;
        }

        if let Some(protocol) = parse_protocol_row(trimmed) {
            protocols.push(protocol);
        }
    }

    protocols
}

fn parse_protocol_row(line: &str) -> Option<Protocol> {
    let mut tokens = line.split_whitespace();
    let name = tokens.next()?.to_string();
    let proto = tokens.next()?.to_string();
    let table = tokens.next()?.to_string();
    let state = tokens.next()?.to_string();

    // "Since" is either a time (13:47:13.517) or a date followed by a time.
    let mut since = tokens.next().map(str::to_string);
    if since.as_deref().is_some_and(looks_like_date)
        && let Some(time) = tokens.next()
    {
        since = since.map(|date| format!("{date} {time}"));
    }

    let info = {
        let rest = tokens.collect::<Vec<_>>().join(" ");
        if rest.is_empty() { None } else { Some(rest) }
    };

    Some(Protocol {
        name,
        proto,
        table: (table != "---").then_some(table),
        state,
        since,
        info,
        attrs: Vec::new(),
        details: Vec::new(),
    })
}

fn looks_like_date(token: &str) -> bool {
    token.len() == 10
        && token.as_bytes()[4] == b'-'
        && token.as_bytes()[7] == b'-'
        && token[..4].chars().all(|c| c.is_ascii_digit())
}

fn parse_attr(line: &str) -> Option<Attr> {
    let value = line.trim_end();
    let indent = (value.len() - value.trim_start().len()).min(u8::MAX as usize) as u8;
    let (key, rest) = value.trim_start().split_once(':')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some(Attr {
        key: key.to_string(),
        value: rest.trim().to_string(),
        indent,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NextHop {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mpls: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub onlink: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Route {
    pub prefix: String,
    /// BIRD's destination type: unicast, blackhole, unreachable, ...
    pub dest: String,
    pub protocol: String,
    pub since: Option<String>,
    pub selected: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sync_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preference: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub next_hops: Vec<NextHop>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<Attr>,
    /// Table name, filled in by the caller that knows the table header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RouteCount {
    pub routes: u64,
    pub total_routes: u64,
    pub networks: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Routes {
    pub routes: Vec<Route>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub counts: Vec<RouteCount>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

pub fn parse_routes(raw: &str) -> Routes {
    let body = strip_banner(raw);
    let mut routes = Routes::default();
    let mut table: Option<String> = None;
    let mut last_prefix: Option<String> = None;

    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Some(name) = line
            .trim()
            .strip_prefix("Table ")
            .and_then(|rest| rest.strip_suffix(':'))
        {
            table = Some(name.trim().to_string());
            last_prefix = None;
            continue;
        }
        if line.trim_start().starts_with("Total: ") {
            if let Some(count) = parse_total_count(line.trim()) {
                routes.counts.push(count);
            }
            continue;
        }
        if let Some(count) = parse_table_count(line.trim(), table.as_deref()) {
            routes.counts.push(count);
            continue;
        }

        if line.starts_with('\t') {
            if let Some(route) = routes.routes.last_mut() {
                parse_route_detail(line, route);
            }
            continue;
        }

        let (prefix, rest) = if line.starts_with(' ') {
            match &last_prefix {
                Some(prefix) => (prefix.clone(), line.trim_start()),
                None => {
                    routes
                        .warnings
                        .push(format!("route line without prefix: {line}"));
                    continue;
                }
            }
        } else {
            let (prefix, rest) = split_token(line);
            last_prefix = Some(prefix.to_string());
            (prefix.to_string(), rest)
        };

        match parse_route_line(&prefix, rest, table.clone()) {
            Some(route) => routes.routes.push(route),
            None => routes.warnings.push(format!("unparsed route line: {line}")),
        }
    }

    routes
}

fn parse_route_line(prefix: &str, rest: &str, table: Option<String>) -> Option<Route> {
    let rest = rest.trim_start();
    let (dest, rest) = split_token(rest);
    if dest.is_empty() {
        return None;
    }

    let open = rest.find('[')?;
    let close = rest[open..].find(']')? + open;
    let inside = rest[open + 1..close].trim();
    let (protocol, since) = match inside.split_once(char::is_whitespace) {
        Some((protocol, since)) => (protocol.to_string(), Some(since.trim().to_string())),
        None => (inside.to_string(), None),
    };

    let tail = rest[close + 1..].trim();
    let (selected, tail) = match tail.strip_prefix('*') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, tail),
    };
    let (sync_error, tail) = match tail.strip_prefix('!') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, tail),
    };
    let (preference, info) = split_preference(tail);

    Some(Route {
        prefix: prefix.to_string(),
        dest: dest.to_string(),
        protocol,
        since,
        selected,
        sync_error,
        preference,
        info: (!info.is_empty()).then_some(info),
        next_hops: Vec::new(),
        attrs: Vec::new(),
        table,
    })
}

fn split_preference(tail: &str) -> (Option<u64>, String) {
    let Some(rest) = tail.strip_prefix('(') else {
        return (None, tail.to_string());
    };
    let Some((number, after)) = rest.split_once(')') else {
        return (None, tail.to_string());
    };
    match number.trim().parse::<u64>() {
        Ok(value) => (Some(value), after.trim().to_string()),
        Err(_) => (None, tail.to_string()),
    }
}

fn parse_route_detail(line: &str, route: &mut Route) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if trimmed.starts_with("via ") || trimmed.starts_with("dev ") {
        if let Some(hop) = parse_next_hop(trimmed) {
            route.next_hops.push(hop);
        }
        return;
    }
    if let Some(attr) = parse_attr(line) {
        route.attrs.push(attr);
    }
}

fn parse_next_hop(line: &str) -> Option<NextHop> {
    let mut tokens = line.split_whitespace().peekable();
    let mut hop = NextHop {
        via: None,
        dev: None,
        mpls: None,
        onlink: false,
        weight: None,
    };
    match tokens.next()? {
        "via" => hop.via = tokens.next().map(str::to_string),
        "dev" => hop.dev = tokens.next().map(str::to_string),
        _ => return None,
    }
    while let Some(token) = tokens.next() {
        match token {
            "on" => hop.dev = tokens.next().map(str::to_string),
            "mpls" => hop.mpls = tokens.next().map(str::to_string),
            "onlink" => hop.onlink = true,
            "weight" => {
                hop.weight = tokens.next().and_then(|value| value.parse().ok());
            }
            _ => {}
        }
    }
    Some(hop)
}

fn parse_table_count(line: &str, table: Option<&str>) -> Option<RouteCount> {
    let mut count = parse_count_numbers(line)?;
    if count.table.is_none() {
        count.table = table.map(str::to_string);
    }
    Some(count)
}

fn parse_total_count(line: &str) -> Option<RouteCount> {
    let rest = line.strip_prefix("Total: ")?;
    parse_count_numbers(rest)
}

/// Parses `"3 of 3 routes for 3 networks ..."` into its three numbers.
fn parse_count_numbers(line: &str) -> Option<RouteCount> {
    let (numbers, rest) = line.split_once(" routes for ")?;
    let (routes, total_routes) = numbers.split_once(" of ")?;
    let (networks, tail) = rest.split_once(" networks")?;
    let table = tail
        .strip_prefix(" in table ")
        .map(|table| table.trim().to_string())
        .filter(|table| !table.is_empty());
    Some(RouteCount {
        routes: routes.trim().parse().ok()?,
        total_routes: total_routes.trim().parse().ok()?,
        networks: networks.trim().parse().ok()?,
        table,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterfaceAddr {
    pub address: String,
    pub prefix_len: u8,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opposite: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Interface {
    pub name: String,
    pub up: bool,
    pub index: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    pub admin_up: bool,
    pub link_up: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<InterfaceAddr>,
}

pub fn parse_interfaces(raw: &str) -> Vec<Interface> {
    let mut interfaces: Vec<Interface> = Vec::new();

    for line in strip_banner(raw).lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('\t') {
            if let Some(interface) = interfaces.last_mut() {
                parse_interface_detail(line, interface);
            }
            continue;
        }
        if let Some(interface) = parse_interface_header(line) {
            interfaces.push(interface);
        }
    }

    interfaces
}

fn parse_interface_header(line: &str) -> Option<Interface> {
    let (name, rest) = split_token(line);
    let (state, rest) = split_token(rest.trim_start());
    if name.is_empty() || state.is_empty() {
        return None;
    }

    let mut index = 0;
    let mut master = None;
    if let Some(start) = rest.find("index=") {
        let after = &rest[start + "index=".len()..];
        let end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        index = after[..end].parse().ok()?;
        if let Some(master_start) = after.find("master=") {
            let value = &after[master_start + "master=".len()..];
            let end = value
                .find(|c: char| c.is_whitespace() || c == ')')
                .unwrap_or(value.len());
            master = Some(value[..end].to_string());
        }
    }

    Some(Interface {
        name: name.to_string(),
        up: state == "up",
        index,
        master,
        kind: None,
        flags: Vec::new(),
        admin_up: false,
        link_up: false,
        mtu: None,
        addresses: Vec::new(),
    })
}

fn parse_interface_detail(line: &str, interface: &mut Interface) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if trimmed.contains("MTU=") {
        for token in trimmed.split_whitespace() {
            if let Some(mtu) = token.strip_prefix("MTU=") {
                interface.mtu = mtu.parse().ok();
            } else {
                match token {
                    "AdminUp" => interface.admin_up = true,
                    "AdminDown" => interface.admin_up = false,
                    "LinkUp" => interface.link_up = true,
                    "LinkDown" => interface.link_up = false,
                    other => {
                        if interface.kind.is_none() && matches!(other, "MultiAccess" | "PtP") {
                            interface.kind = Some(other.to_string());
                        } else {
                            interface.flags.push(other.to_string());
                        }
                    }
                }
            }
        }
        return;
    }

    if let Some(address) = parse_interface_address(trimmed) {
        interface.addresses.push(address);
    }
}

fn parse_interface_address(line: &str) -> Option<InterfaceAddr> {
    let (address, rest) = line.split_once(" (")?;
    let rest = rest.trim_end_matches(')');
    let (address, prefix_len) = address.trim().rsplit_once('/')?;
    let mut flags = Vec::new();
    let mut scope = None;
    let mut opposite = None;
    for part in rest.split(", ").map(str::trim) {
        if let Some(value) = part.strip_prefix("scope ") {
            scope = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("opposite ") {
            opposite = Some(value.to_string());
        } else if !part.is_empty() {
            flags.push(part.to_string());
        }
    }
    Some(InterfaceAddr {
        address: address.to_string(),
        prefix_len: prefix_len.parse().ok()?,
        flags,
        scope,
        opposite,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterfaceSummary {
    pub name: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<String>,
}

pub fn parse_interface_summary(raw: &str) -> Vec<InterfaceSummary> {
    let body = strip_banner(raw);
    let mut lines = body.lines().filter(|line| !line.trim().is_empty());
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let columns: Vec<usize> = ["Interface", "State", "IPv4 address", "IPv6 address"]
        .iter()
        .filter_map(|name| header.find(name))
        .collect();
    if columns.len() < 4 {
        return Vec::new();
    }

    let mut summary = Vec::new();
    for line in lines {
        let slice = |start: usize, end: usize| line.get(start..end).unwrap_or("").trim();
        let ipv4 = slice(columns[2], columns[3]);
        let ipv6 = line.get(columns[3]..).unwrap_or("").trim();
        summary.push(InterfaceSummary {
            name: slice(columns[0], columns[1]).to_string(),
            state: slice(columns[1], columns[2]).to_string(),
            ipv4: (!ipv4.is_empty()).then(|| ipv4.to_string()),
            ipv6: (!ipv6.is_empty()).then(|| ipv6.to_string()),
        });
    }
    summary
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Symbol {
    pub name: String,
    pub class: String,
}

pub fn parse_symbols(raw: &str) -> Vec<Symbol> {
    strip_banner(raw)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (name, class) = line.split_once('\t').or_else(|| line.split_once(' '))?;
            Some(Symbol {
                name: name.trim().to_string(),
                class: class.trim().to_string(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigCheck {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

pub fn parse_config_check(raw: &str) -> ConfigCheck {
    let body = strip_banner(raw);
    let mut file = None;
    let mut ok = false;
    let mut errors = Vec::new();

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(path) = line.strip_prefix("Reading configuration from ") {
            file = Some(path.trim().to_string());
        } else if line == "Configuration OK" {
            ok = true;
        } else if line.starts_with("Reading configuration from") {
            continue;
        } else {
            errors.push(line.to_string());
        }
    }

    let message = if ok {
        "Configuration OK".to_string()
    } else if let Some(first) = errors.first() {
        first.clone()
    } else {
        "configuration check returned no verdict".to_string()
    };

    ConfigCheck {
        ok,
        file,
        message,
        errors,
    }
}

fn split_token(input: &str) -> (&str, &str) {
    let input = input.trim_start();
    match input.find(char::is_whitespace) {
        Some(index) => (&input[..index], &input[index..]),
        None => (input, ""),
    }
}
