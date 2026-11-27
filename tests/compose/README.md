# Five-AS BIRD lab with traffic and an OpenCode MCP client

The point of this setup is the `opencode` container: it has `birdc`, `bird-mcp`
and OpenCode, and reaches **all five routers** through one MCP server. The
routers, hosts and traffic generator exist so the agent has something real to
inspect and change.

## Quick start

```sh
cd tests/compose
docker compose up -d --build                    # first build takes ~3 minutes
docker compose exec -it opencode opencode auth login   # once per provider
docker compose exec -it opencode opencode
```

`opencode mcp list` should show `✓ bird connected`. The workdir contains an
`AGENTS.md` describing the lab; the MCP server is called `bird` and its tools
take `router: "r1"` … `"r5"`.

The default model is `opencode-go/deepseek-v4.1-flash` (set in
`agent/opencode.json`). Change it with `/models` in the TUI or by editing that
file and recreating the container. Without a model in the config, OpenCode picks
its own default (currently a free `hungerg-free` model), not your Go plan.

Credentials (including OpenCode Go) live in `/root/.local/share/opencode` inside
the container, which is the named volume `opencode_auth`. OpenCode v2 stores
them in the SQLite database `opencode.db` (table `credential`; legacy
`auth.json` from v1 is imported on first start), so they survive container
recreation. `docker compose down` keeps the volume; `docker compose down -v`
deletes it.

### Use your host OpenCode Go login

Pick one:

```sh
# 1. API key: put OPENCODE_API_KEY=... into tests/compose/.env
cp .env.example .env && $EDITOR .env

# 2. Extract the host's OpenCode Go key into .env and recreate the container
./use-host-login.sh

# 3. Reuse the whole host data directory live
OPENCODE_DATA_DIR=$HOME/.local/share/opencode docker compose up -d --force-recreate opencode
```

After option 1, recreate the container (`docker compose up -d --force-recreate
opencode`); option 2 does that itself. `opencode auth list` inside the container
should list OpenCode Go. Option 3 shares the SQLite database with the host: do
not run host OpenCode and the container at the same time, and expect the
(large) database to be accessed through the mount.

### Container tooling

Besides `birdc`, `bird-mcp` and `opencode`, the agent image carries a network
toolbox: `mtr`, `traceroute`, `tracepath`, `ping`, `fping`, `arping`, `dig`,
`nc`, `socat`, `tcpdump`, `iperf3`, `ethtool` and `whois`.

`tcpdump` only sees the opencode container's own interfaces (the compose
default network). To capture a lab link, run it inside the host or router
network, for example `docker compose exec h1 tcpdump -i eth0 -nn`.

## What to try in OpenCode

Just ask in natural language; the agent picks the MCP tools.

| Prompt | What happens |
| --- | --- |
| "Show status and all BGP sessions on the five routers." | `bird_fleet` status + `bird_fleet` protocols with `details=true` |
| "Which path does r1 use for 10.10.4.0/24, and why?" | `bird_routes` with `match_mode: "for"`, `verbose: true` incl. AS path |
| "Blackhole 10.10.3.0/24 on r1." | `bird_routes_edit` add → config check → reload → verification |
| "Remove that blackhole again." | `bird_routes_edit` remove |
| "Restart peer3 on r2." | `bird_protocol_control` restart |
| "Check the config on r5 without applying it." | `bird_configure` with `mode: "check"` |
| "What does the BIRD documentation say about next hop self?" | `bird_docs_search` / `bird_docs_section` |

The same calls as raw arguments, if you want to see what the agent sends:

```jsonc
bird_routers          { "probe": true }
bird_status           { "router": "r3" }
bird_protocols        { "router": "r2", "details": true }
bird_routes           { "router": "r1", "prefix": "10.10.4.0/24", "match_mode": "for", "verbose": true }
bird_routes_edit      { "router": "r1", "action": "add", "prefix": "10.10.3.0/24" }
bird_routes_edit      { "router": "r1", "action": "remove", "prefix": "10.10.3.0/24" }
bird_protocol_control { "router": "r2", "action": "restart", "target": "peer3" }
bird_configure        { "router": "r5", "mode": "check" }
bird_show             { "router": "r4", "command": "show ospf neighbors" }
bird_docs_search      { "query": "next hop self" }
```

### Watch the blackhole land

```sh
docker compose logs -f sim
```

Before: `dst=10.10.3.3 ping=up http=200`. After the blackhole on r1:
`ping=DOWN http=000` within a few seconds, while every other destination stays
up. Removing it restores traffic. The sim also prints an iperf3 sample to
AS64515 once a minute.

## Topology

<p align="center">
  <img src="docs/topology.svg?v=2" width="1000"
       alt="Five BIRD routers in an eBGP ring, one per AS, each with a customer prefix and host; a traffic generator sits behind r1">
</p>

Each router originates its customer prefix (10.10.N.0/24), exports static and
BGP routes over eBGP and installs BGP routes in the kernel, so traffic really
traverses the ring. Hosts are `10.10.N.3` behind router N; `sim` is
`10.10.1.4` behind r1.

### Lab hostnames and DNS

Every router, host and the traffic generator resolves the others by name, and
reverse lookups work. A small dnsmasq service (`dns.lab`, `10.99.0.53`) serves
the lab zone and the PTR records; all containers use it and forward everything
else to the public resolvers. The same names are also in `/etc/hosts` as a
fallback:

| Name | Address |
| --- | --- |
| `gw.as64512.lab` … `gw.as64516.lab` | `10.10.N.2` (router / gateway of AS6451N) |
| `host.as64512.lab` … `host.as64516.lab` | `10.10.N.3` (customer host in AS6451N) |
| `sim.as64512.lab` | `10.10.1.4` (traffic generator behind AS64512) |
| `r1.lab` … `r5.lab`, `h1.lab` … `h5.lab`, `sim.lab` | short aliases for the same addresses |
| `r1-link12.lab`, `r2-link12.lab` | `10.0.12.2`, `10.0.12.3` |
| `r2-link23.lab`, `r3-link23.lab` | `10.0.23.2`, `10.0.23.3` |
| `r3-link34.lab`, `r4-link34.lab` | `10.0.34.2`, `10.0.34.3` |
| `r4-link45.lab`, `r5-link45.lab` | `10.0.45.2`, `10.0.45.3` |
| `r5-link51.lab`, `r1-link51.lab` | `10.0.51.2`, `10.0.51.3` |
| `docker-link12.lab` … `docker-link51.lab` | `10.0.X.1` (Docker bridge gateway) |
| `docker-as1.lab` … `docker-as5.lab` | `10.10.N.1` (Docker bridge gateway) |
| `docker-mgmt.lab` | `10.99.0.1` (Docker bridge gateway) |

```sh
docker compose exec h1 ping -c1 gw.as64515.lab
docker compose exec h1 traceroute gw.as64516.lab
docker compose exec h1 dig +short host.as64514.lab
docker compose exec h1 dig -x 10.10.3.2 +short      # gw.as64514.lab
docker compose exec opencode dig -x 10.10.1.2 +short # gw.as64512.lab
```

Within each subnet, `.1` is Docker's bridge gateway (`docker-…​.lab`) and `.2`
is the BIRD router (`gw.as645NN.lab`). The gateways are the Docker VM, not BIRD;
naming them keeps traceroute output unambiguous.

The OpenCode container is also attached to `as1` (`10.10.1.10`) and routes
`10.0.0.0/16` and `10.10.0.0/16` via r1, so `ping`, `traceroute` and `mtr`
reach the lab from the agent container as well:

```sh
docker compose exec opencode traceroute gw.as64516.lab
# 1  10.10.1.2        (r1)
# 2  10.10.5.2        (r5)
```

Docker's internal resolver names the first hop after the container
(`birdlab-r1…`); `traceroute -n` shows the address instead.

### MCP wiring

One MCP server (`bird`) exposes all five routers through the `router` argument.
Every router is write-enabled and already includes the file that
`bird_routes_edit` manages:

| MCP router | AS | BIRD include | routes_file seen by the agent |
| --- | --- | --- | --- |
| `r1` | 64512 | `/etc/bird/state/bird-mcp.conf` | `/state/r1/bird-mcp.conf` |
| `r2` | 64513 | `/etc/bird/state/bird-mcp.conf` | `/state/r2/bird-mcp.conf` |
| `r3` | 64514 | `/etc/bird/state/bird-mcp.conf` | `/state/r3/bird-mcp.conf` |
| `r4` | 64515 | `/etc/bird/state/bird-mcp.conf` | `/state/r4/bird-mcp.conf` |
| `r5` | 64516 | `/etc/bird/state/bird-mcp.conf` | `/state/r5/bird-mcp.conf` |

Add and remove were exercised on each of the five routers.

## WHOIS data

A small fictional RIPE-style WHOIS server runs as the compose service `whois`
with the hostname `whois.lab`. It serves static `inetnum` objects for the five
customer prefixes and the five eBGP links from `whois/data.txt` — port 43 on the
lab network, `localhost:4343` on the host.

The records look like a registry export but are deliberately fictional:
`admin-c`/`tech-c` are `LAB-ROPE` and `source` is `ROPE`, not `RIPE`.

| inetnum | netname | object |
| --- | --- | --- |
| `10.10.1.0 - 10.10.1.255` | `LAB-AS64512` | customer prefix r1 (AS64512) |
| `10.10.2.0 - 10.10.2.255` | `LAB-AS64513` | customer prefix r2 (AS64513) |
| `10.10.3.0 - 10.10.3.255` | `LAB-AS64514` | customer prefix r3 (AS64514) |
| `10.10.4.0 - 10.10.4.255` | `LAB-AS64515` | customer prefix r4 (AS64515) |
| `10.10.5.0 - 10.10.5.255` | `LAB-AS64516` | customer prefix r5 (AS64516) |
| `10.0.12.0 - 10.0.12.7` | `LAB-LINK-R1-R2` | eBGP link r1 – r2 |
| `10.0.23.0 - 10.0.23.7` | `LAB-LINK-R2-R3` | eBGP link r2 – r3 |
| `10.0.34.0 - 10.0.34.7` | `LAB-LINK-R3-R4` | eBGP link r3 – r4 |
| `10.0.45.0 - 10.0.45.7` | `LAB-LINK-R4-R5` | eBGP link r4 – r5 |
| `10.0.51.0 - 10.0.51.7` | `LAB-LINK-R5-R1` | eBGP link r5 – r1 |

Each `/24` also has a fictional reverse-zone object (`domain:`), for example
`3.10.10.in-addr.arpa` with `nserver: dns.lab 10.99.0.53`. An address query
returns the inetnum plus its zone, and zones can be queried by name. The actual
PTR answers come from `dns.lab`; the WHOIS objects are the registry metadata.

```sh
# from the OpenCode container
docker compose exec opencode whois -h whois.lab 10.10.3.0
docker compose exec opencode whois -h whois.lab 10.10.3.0/24
docker compose exec opencode whois -h whois.lab 10.0.12.0
docker compose exec opencode whois -h whois.lab 3.10.10.in-addr.arpa
docker compose exec opencode whois -h whois.lab 12.0.10.in-addr.arpa

# from the host (macOS/Linux whois client)
whois -h 127.0.0.1 -p 4343 10.10.4.0
```

Example response:

```
% This is the BIRD-MCP lab WHOIS server (fictional).
% Query: 10.10.3.0

inetnum:        10.10.3.0 - 10.10.3.255
netname:        LAB-AS64514
descr:          BIRD-MCP lab customer prefix for AS64514
country:        DE
admin-c:        LAB-ROPE
tech-c:         LAB-ROPE
status:         ASSIGNED PA
...
source:         ROPE
```

Querying an address without an object returns `%ERROR:101: no entries found`.
The data is static: edit `whois/data.txt` and run `docker compose restart whois`.

## Optional: shell checks without OpenCode

Only needed for debugging the lab itself.

```sh
docker compose exec opencode bird-mcp --check
docker compose exec opencode birdc -s /run/bird/r3/bird.ctl show protocols
docker compose exec h1 curl -s http://10.10.5.3/
```

## Stop

```sh
docker compose down          # keeps the opencode_auth volume with your logins
# docker compose down -v    # also deletes OpenCode credentials and socket volumes
rm -rf state
```

## Notes

- Lab, not a deployment guide: all routers are write-enabled, routes are not
  filtered, and the agent container shares the control sockets.
- Routers need `NET_ADMIN` for the kernel protocol and forwarding; hosts need it
  to point their default route at the router.
- The BIRD checkout in `../../bird` (branch `v3.4`) is required for the builds.
