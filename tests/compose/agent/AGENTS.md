# BIRD lab — notes for the agent

You are attached to a five-router BIRD lab through the `bird` MCP server. Every
router is a separate entry in the `router` argument: `r1` … `r5`.

## Topology

| Router | AS | Customer prefix | Router ID | Links |
| --- | --- | --- | --- | --- |
| r1 | 64512 | 10.10.1.0/24 | 10.255.0.1 | r2 (10.0.12.0/29), r5 (10.0.51.0/29) |
| r2 | 64513 | 10.10.2.0/24 | 10.255.0.2 | r1, r3 (10.0.23.0/29) |
| r3 | 64514 | 10.10.3.0/24 | 10.255.0.3 | r2, r4 (10.0.34.0/29) |
| r4 | 64515 | 10.10.4.0/24 | 10.255.0.4 | r3, r5 (10.0.45.0/29) |
| r5 | 64516 | 10.10.5.0/24 | 10.255.0.5 | r4, r1 |

The ASes form a ring. Each router originates its customer prefix with a static
blackhole route, exports static routes to its eBGP peers and installs BGP routes
into the kernel (`protocol kernel { ipv4 { import none; export where source =
RTS_BGP; }; }`), so traffic is actually forwarded.

Hosts `10.10.N.3` sit behind router N; the traffic generator `10.10.1.4` pings
and fetches HTTP from all other hosts every few seconds.

## Useful tool calls

- `bird_routers` — see all five routers and whether they answer.
- `bird_fleet { "check": "status" }` — one parallel query across all routers
  (also `protocols`, `routes`, `interfaces`); results carry per-router errors.
- `bird_status { router: "r3" }` — version, uptime, last reconfiguration.
- `bird_protocols { router: "r2", details: true }` — BGP session state and counters.
- `bird_routes { router: "r1", verbose: true }` — full table with AS paths.
- `bird_routes { router: "r1", prefix: "10.10.3.0/24", match_mode: "for" }` —
  which path r1 would use for that destination.
- `bird_show { router: "r4", command: "show symbols" }` — anything else read-only.
- `bird_routes_edit` — add or remove routes on a router. Example:
  `{"router":"r1","action":"add","prefix":"10.10.3.0/24"}` blackholes that
  prefix on r1 and announces it to the ring (remote blackhole). Remove it with
  `action: "remove"`.

## Trying things

1. Ask `bird_routes` on each router and compare the AS paths.
2. Add a blackhole for `10.10.3.0/24` on r1 and watch the route disappear from
   the other routers' tables; the human can follow it in `docker compose logs -f sim`.
3. Remove the blackhole again and confirm reachability returns.
4. Use `bird_configure { router: "r5", mode: "check" }` before changing config.
5. `bird_docs_search` and `bird_docs_section` read the BIRD 3.4 documentation
   shipped with the checkout.

Every router is write-enabled in this lab (`read_only = false`). Do not treat
these settings as production-safe.
