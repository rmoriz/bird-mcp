use bird_mcp::parse::{
    parse_config_check, parse_interface_summary, parse_interfaces, parse_protocols, parse_routes,
    parse_status, parse_symbols, strip_banner,
};

#[test]
fn banner_is_stripped() {
    assert_eq!(strip_banner("BIRD 3.4.0 ready.\nhello\n"), "hello\n");
    assert_eq!(strip_banner("hello\n"), "hello\n");
    assert_eq!(strip_banner("BIRD 3.4.0 ready."), "BIRD 3.4.0 ready.");
}

#[test]
fn status_is_parsed() {
    let status = parse_status(include_str!("fixtures/status.txt"));
    assert_eq!(status.version, "3.4.0");
    assert_eq!(status.router_id.as_deref(), Some("10.255.0.1"));
    assert!(status.hostname.is_some());
    assert!(status.server_time.is_some());
    assert!(status.last_reboot.is_some());
    assert!(status.last_reconfiguration.is_some());
    assert_eq!(status.state.as_deref(), Some("Daemon is up and running"));
    assert!(status.other.is_empty());
}

#[test]
fn protocols_are_parsed() {
    let protocols = parse_protocols(include_str!("fixtures/protocols.txt"));
    assert_eq!(protocols.len(), 3);

    let device = &protocols[0];
    assert_eq!(device.name, "device1");
    assert_eq!(device.proto, "Device");
    assert_eq!(device.table, None);
    assert_eq!(device.state, "up");

    let static4 = &protocols[1];
    assert_eq!(static4.name, "static4");
    assert_eq!(static4.table.as_deref(), Some("master4"));
    assert!(
        static4
            .since
            .as_deref()
            .is_some_and(|since| since.contains(':'))
    );

    let bgp = &protocols[2];
    assert_eq!(bgp.name, "peer2");
    assert_eq!(bgp.proto, "BGP");
    assert_eq!(bgp.info.as_deref(), Some("Established"));
    assert!(bgp.details.is_empty());
}

#[test]
fn protocol_details_become_attributes() {
    let protocols = parse_protocols(include_str!("fixtures/protocols-all.txt"));
    let bgp = protocols
        .iter()
        .find(|protocol| protocol.name == "peer2")
        .expect("peer2 in protocol list");

    assert!(bgp.details.len() > 20);
    let attr = |key: &str| {
        bgp.attrs
            .iter()
            .find(|attr| attr.key == key)
            .map(|attr| attr.value.as_str())
    };
    assert_eq!(attr("BGP state"), Some("Established"));
    assert_eq!(attr("Neighbor address"), Some("172.30.0.12"));
    assert_eq!(attr("Neighbor AS"), Some("64513"));
    assert_eq!(attr("Table"), Some("master4"));
    assert!(attr("Hold timer").is_some());
}

#[test]
fn single_protocol_details_are_parsed() {
    let protocols = parse_protocols(include_str!("fixtures/protocol-peer2.txt"));
    assert_eq!(protocols.len(), 1);
    let bgp = &protocols[0];
    assert_eq!(bgp.name, "peer2");
    assert_eq!(bgp.state, "up");
    assert!(
        bgp.attrs
            .iter()
            .any(|attr| attr.key == "Routes" && attr.value.contains("imported"))
    );
}

#[test]
fn routes_are_parsed_with_prefix_inheritance() {
    let routes = parse_routes(include_str!("fixtures/route.txt"));
    assert!(routes.warnings.is_empty());
    assert_eq!(routes.routes.len(), 5);

    let blackhole = &routes.routes[0];
    assert_eq!(blackhole.prefix, "192.0.2.0/24");
    assert_eq!(blackhole.dest, "blackhole");
    assert_eq!(blackhole.protocol, "static4");
    assert!(blackhole.selected);
    assert_eq!(blackhole.preference, Some(200));
    assert_eq!(blackhole.table.as_deref(), Some("master4"));

    // The second route has an empty prefix column and inherits 192.0.2.0/24.
    let inherited = &routes.routes[1];
    assert_eq!(inherited.prefix, "192.0.2.0/24");
    assert_eq!(inherited.dest, "unicast");
    assert!(!inherited.selected);
    assert_eq!(inherited.next_hops.len(), 1);
    assert_eq!(inherited.next_hops[0].via.as_deref(), Some("172.30.0.1"));
    assert_eq!(inherited.next_hops[0].dev.as_deref(), Some("eth0"));

    let bgp = &routes.routes[4];
    assert_eq!(bgp.prefix, "203.0.113.0/24");
    assert_eq!(bgp.protocol, "peer2");
    assert_eq!(bgp.info.as_deref(), Some("[AS64513i]"));
    assert!(!bgp.selected);
}

#[test]
fn verbose_routes_keep_attributes() {
    let routes = parse_routes(include_str!("fixtures/route-all.txt"));
    assert_eq!(routes.routes.len(), 5);

    let bgp = routes
        .routes
        .iter()
        .find(|route| route.protocol == "peer2")
        .expect("bgp route");
    let attr = |key: &str| {
        bgp.attrs
            .iter()
            .find(|attr| attr.key == key)
            .map(|attr| attr.value.as_str())
    };
    assert_eq!(attr("bgp_path"), Some("64513"));
    assert_eq!(attr("bgp_origin"), Some("IGP"));
    assert_eq!(attr("preference"), Some("100"));
    assert_eq!(attr("source"), Some("BGP"));
}

#[test]
fn route_counts_are_parsed() {
    let routes = parse_routes(include_str!("fixtures/route-count.txt"));
    assert!(routes.routes.is_empty());
    assert_eq!(routes.counts.len(), 3);

    let master4 = routes
        .counts
        .iter()
        .find(|count| count.table.as_deref() == Some("master4"))
        .expect("master4 count");
    assert_eq!(master4.routes, 5);
    assert_eq!(master4.total_routes, 5);
    assert_eq!(master4.networks, 3);

    let total = routes
        .counts
        .iter()
        .find(|count| count.table.is_none())
        .expect("total count");
    assert_eq!(total.routes, 5);
}

#[test]
fn forwarding_lookup_is_parsed() {
    let routes = parse_routes(include_str!("fixtures/route-for.txt"));
    assert_eq!(routes.routes.len(), 2);
    assert_eq!(routes.routes[0].prefix, "203.0.113.0/24");
    assert!(routes.routes[0].selected);
    assert_eq!(routes.routes[1].prefix, "203.0.113.0/24");
    assert_eq!(routes.routes[1].protocol, "peer2");
    assert!(!routes.routes[1].selected);
}

#[test]
fn interfaces_are_parsed() {
    let interfaces = parse_interfaces(include_str!("fixtures/interfaces.txt"));
    assert!(!interfaces.is_empty());

    let lo = interfaces
        .iter()
        .find(|interface| interface.name == "lo")
        .expect("lo");
    assert!(lo.up);
    assert_eq!(lo.index, 1);
    assert_eq!(lo.kind.as_deref(), Some("MultiAccess"));
    assert!(lo.flags.iter().any(|flag| flag == "Loopback"));
    assert!(lo.flags.iter().any(|flag| flag == "Ignored"));
    assert_eq!(lo.addresses.len(), 2);
    assert_eq!(lo.addresses[0].address, "127.0.0.1");
    assert_eq!(lo.addresses[0].prefix_len, 8);
    assert_eq!(lo.addresses[0].scope.as_deref(), Some("host"));

    let eth0 = interfaces
        .iter()
        .find(|interface| interface.name == "eth0")
        .expect("eth0");
    assert!(eth0.up);
    assert_eq!(eth0.mtu, Some(1500));
    assert!(eth0.admin_up);
    assert!(eth0.link_up);
    assert_eq!(eth0.addresses[0].address, "172.30.0.11");
    assert_eq!(eth0.addresses[0].prefix_len, 24);
}

#[test]
fn interface_summary_columns_are_parsed() {
    let summary = parse_interface_summary(include_str!("fixtures/interfaces-summary.txt"));
    assert!(!summary.is_empty());

    let lo = summary.iter().find(|line| line.name == "lo").expect("lo");
    assert_eq!(lo.state, "up");
    assert_eq!(lo.ipv4.as_deref(), Some("127.0.0.1/8"));
    assert_eq!(lo.ipv6.as_deref(), Some("::1/128"));

    let eth0 = summary
        .iter()
        .find(|line| line.name == "eth0")
        .expect("eth0");
    assert_eq!(eth0.ipv4.as_deref(), Some("172.30.0.11/24"));
    assert_eq!(eth0.ipv6, None);
}

#[test]
fn symbols_are_parsed() {
    let symbols = parse_symbols(include_str!("fixtures/symbols.txt"));
    assert!(!symbols.is_empty());
    assert!(
        symbols
            .iter()
            .any(|symbol| symbol.name == "master4" && symbol.class == "routing table")
    );
    assert!(
        symbols
            .iter()
            .any(|symbol| symbol.name == "peer2" && symbol.class == "protocol")
    );
}

#[test]
fn successful_config_check_is_parsed() {
    let check = parse_config_check(include_str!("fixtures/config-check.txt"));
    assert!(check.ok);
    assert_eq!(check.message, "Configuration OK");
    assert_eq!(check.file.as_deref(), Some("/etc/bird/bird.conf"));
    assert!(check.errors.is_empty());
}

#[test]
fn failed_config_check_is_parsed() {
    let check = parse_config_check(include_str!("fixtures/config-check-error.txt"));
    assert!(!check.ok);
    assert_eq!(check.file.as_deref(), Some("/etc/bird/bad.conf"));
    assert!(
        check
            .errors
            .iter()
            .any(|error| error.contains("syntax error"))
    );
    assert!(check.message.contains("syntax error"));
}
