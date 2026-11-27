//! Reading and writing the managed static-route include file.
//!
//! BIRD has no runtime command to add or remove routes: a route change is a
//! configuration change. This module keeps a small include file that the
//! operator pulls into `bird.conf`, for example
//!
//! ```text
//! include "/etc/bird/bird-mcp.conf";
//! ```
//!
//! and renders it as two static protocols (one per address family) so the MCP
//! tool can add, remove and list routes and then reload BIRD.

use serde::Serialize;

pub const PROTOCOL_V4: &str = "bird_mcp_static4";
pub const PROTOCOL_V6: &str = "bird_mcp_static6";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RouteKind {
    /// Silently drop packets for the prefix (classic blackhole / RTBH next hop).
    Blackhole,
    /// Answer with ICMP unreachable.
    Unreachable,
    /// Answer with ICMP administratively prohibited.
    Prohibit,
    /// Forward towards a gateway and/or out of an interface.
    Via {
        #[serde(skip_serializing_if = "Option::is_none")]
        gateway: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        dev: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaticRoute {
    pub prefix: String,
    pub family: &'static str,
    #[serde(flatten)]
    pub kind: RouteKind,
}

impl StaticRoute {
    pub fn new(prefix: String, kind: RouteKind) -> StaticRoute {
        let family = if prefix.contains(':') { "ipv6" } else { "ipv4" };
        StaticRoute {
            prefix,
            family,
            kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutesFile {
    pub routes: Vec<StaticRoute>,
    pub warnings: Vec<String>,
}

/// Parses the route statements of a managed include file. Everything that is
/// not a `route ...;` statement (protocol blocks, comments, preference) is
/// generated again by [`render`]; unknown route syntax is reported.
pub fn parse(content: &str) -> RoutesFile {
    let mut routes = Vec::new();
    let mut warnings = Vec::new();

    for (index, line) in content.lines().enumerate() {
        let line = strip_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        let Some(rest) = line.strip_prefix("route ") else {
            if line.starts_with("route\t") {
                warnings.push(format!("line {}: route statement uses a tab", index + 1));
            }
            continue;
        };
        match parse_route(rest) {
            Some(route) => routes.push(route),
            None => warnings.push(format!("line {}: unparsed route: {line}", index + 1)),
        }
    }

    RoutesFile { routes, warnings }
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, character) in line.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_route(rest: &str) -> Option<StaticRoute> {
    let rest = rest.trim().trim_end_matches(';');
    let tokens = tokenize(rest);
    let prefix = tokens.first()?.trim().to_string();
    if prefix.is_empty() {
        return None;
    }
    let words = &tokens[1..];

    let kind = if has_word(words, "blackhole") || has_word(words, "drop") {
        RouteKind::Blackhole
    } else if has_word(words, "unreachable") || has_word(words, "reject") {
        RouteKind::Unreachable
    } else if has_word(words, "prohibit") {
        RouteKind::Prohibit
    } else {
        parse_via(words)?
    };

    Some(StaticRoute::new(prefix, kind))
}

fn parse_via(words: &[String]) -> Option<RouteKind> {
    let position = words.iter().position(|word| word == "via")?;
    let target = words.get(position + 1)?;
    let dev = words
        .iter()
        .position(|word| word == "dev")
        .and_then(|index| words.get(index + 1))
        .cloned();
    if looks_like_ip(target) {
        Some(RouteKind::Via {
            gateway: Some(target.clone()),
            dev,
        })
    } else {
        Some(RouteKind::Via {
            gateway: None,
            dev: Some(target.clone()),
        })
    }
}

fn has_word(words: &[String], wanted: &str) -> bool {
    words.iter().any(|word| word.eq_ignore_ascii_case(wanted))
}

fn looks_like_ip(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == ':' || character == '.')
        && (value.contains('.') || value.contains(':'))
}

/// Splits a BIRD route statement into tokens, keeping `"interface"` together.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for character in input.chars() {
        match character {
            '"' => quoted = !quoted,
            character if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            character => current.push(character),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Renders the complete include file. Routes keep their order; IPv4 and IPv6
/// get separate static protocols because BIRD gives a static protocol exactly
/// one channel.
pub fn render(routes: &[StaticRoute], preference: u32) -> String {
    let mut out = String::new();
    out.push_str(
        "# Managed by bird-mcp. Changes are overwritten by the bird_routes_edit tool.\n\
         #\n\
         # Pull this file into bird.conf (top level or inside a protocol):\n\
         #   include \"/etc/bird/bird-mcp.conf\";\n\
         #\n\
         # The routes live in the static protocols bird_mcp_static4 and\n\
         # bird_mcp_static6; export them in your kernel/BGP filters as needed.\n",
    );

    for (family, protocol, routes) in [
        (
            "ipv4",
            PROTOCOL_V4,
            routes
                .iter()
                .filter(|route| route.family == "ipv4")
                .collect::<Vec<_>>(),
        ),
        (
            "ipv6",
            PROTOCOL_V6,
            routes
                .iter()
                .filter(|route| route.family == "ipv6")
                .collect::<Vec<_>>(),
        ),
    ] {
        if routes.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "\nprotocol static {protocol} {{\n  {family} {{\n    preference {preference};\n  }};\n"
        ));
        for route in routes {
            out.push_str("  ");
            out.push_str(&render_route(route));
            out.push('\n');
        }
        out.push_str("}\n");
    }

    out
}

fn render_route(route: &StaticRoute) -> String {
    match &route.kind {
        RouteKind::Blackhole => format!("route {} blackhole;", route.prefix),
        RouteKind::Unreachable => format!("route {} unreachable;", route.prefix),
        RouteKind::Prohibit => format!("route {} prohibit;", route.prefix),
        RouteKind::Via { gateway, dev } => match (gateway, dev) {
            (Some(gateway), Some(dev)) => {
                format!("route {} via {gateway} dev {dev};", route.prefix)
            }
            (Some(gateway), None) => format!("route {} via {gateway};", route.prefix),
            (None, Some(dev)) => format!("route {} via \"{dev}\";", route.prefix),
            (None, None) => format!("route {} blackhole;", route.prefix),
        },
    }
}

/// Adds a route. Returns false when an identical route already exists.
pub fn add(routes: &mut Vec<StaticRoute>, route: StaticRoute) -> bool {
    if routes.contains(&route) {
        return false;
    }
    routes.push(route);
    true
}

/// Removes every route for one prefix and returns how many were removed.
pub fn remove(routes: &mut Vec<StaticRoute>, prefix: &str) -> usize {
    let before = routes.len();
    routes.retain(|route| route.prefix != prefix);
    before - routes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blackhole(prefix: &str) -> StaticRoute {
        StaticRoute::new(prefix.to_string(), RouteKind::Blackhole)
    }

    #[test]
    fn renders_both_families() {
        let routes = vec![
            blackhole("203.0.113.0/24"),
            StaticRoute::new(
                "2001:db8::/32".to_string(),
                RouteKind::Via {
                    gateway: Some("2001:db8::1".to_string()),
                    dev: Some("eth0".to_string()),
                },
            ),
        ];
        let text = render(&routes, 200);
        assert!(
            text.contains(
                "protocol static bird_mcp_static4 {\n  ipv4 {\n    preference 200;\n  };"
            )
        );
        assert!(text.contains("route 203.0.113.0/24 blackhole;"));
        assert!(text.contains("protocol static bird_mcp_static6 {\n  ipv6 {"));
        assert!(text.contains("route 2001:db8::/32 via 2001:db8::1 dev eth0;"));
    }

    #[test]
    fn empty_file_is_only_a_header() {
        let text = render(&[], 200);
        assert!(!text.contains("protocol static"));
        assert!(text.contains("include \"/etc/bird/bird-mcp.conf\";"));
    }

    #[test]
    fn round_trips_every_kind() {
        let routes = vec![
            blackhole("192.0.2.0/24"),
            StaticRoute::new("198.51.100.0/24".to_string(), RouteKind::Unreachable),
            StaticRoute::new("203.0.113.0/24".to_string(), RouteKind::Prohibit),
            StaticRoute::new(
                "10.0.0.0/8".to_string(),
                RouteKind::Via {
                    gateway: Some("192.0.2.1".to_string()),
                    dev: None,
                },
            ),
            StaticRoute::new(
                "10.1.0.0/16".to_string(),
                RouteKind::Via {
                    gateway: None,
                    dev: Some("eth1".to_string()),
                },
            ),
        ];
        let parsed = parse(&render(&routes, 200));
        assert_eq!(parsed.routes, routes);
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn parses_drop_and_reject_aliases() {
        let parsed = parse("route 192.0.2.0/24 drop;\nroute 198.51.100.0/24 reject;\n");
        assert_eq!(parsed.routes.len(), 2);
        assert_eq!(parsed.routes[0].kind, RouteKind::Blackhole);
        assert_eq!(parsed.routes[1].kind, RouteKind::Unreachable);
    }

    #[test]
    fn comments_and_blocks_are_ignored() {
        let parsed = parse(
            "# comment\nprotocol static x {\n  ipv4;\n  preference 200;\n  route 192.0.2.0/24 blackhole; # inline\n}\n",
        );
        assert_eq!(parsed.routes, vec![blackhole("192.0.2.0/24")]);
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn unknown_route_syntax_is_reported() {
        let parsed = parse("route 192.0.2.0/24 recursive 198.51.100.1;\n");
        assert!(parsed.routes.is_empty());
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn duplicate_is_not_added_twice() {
        let mut routes = vec![blackhole("192.0.2.0/24")];
        assert!(!add(&mut routes, blackhole("192.0.2.0/24")));
        assert_eq!(routes.len(), 1);
    }

    #[test]
    fn remove_by_prefix_removes_all_flavours() {
        let mut routes = vec![
            blackhole("192.0.2.0/24"),
            StaticRoute::new("192.0.2.0/24".to_string(), RouteKind::Prohibit),
            blackhole("198.51.100.0/24"),
        ];
        assert_eq!(remove(&mut routes, "192.0.2.0/24"), 2);
        assert_eq!(routes.len(), 1);
    }
}
