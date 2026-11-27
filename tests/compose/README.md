# Five-AS BIRD lab with traffic and an OpenCode MCP client

A manual test lab: five BIRD 3.4 routers (one per AS) in a ring, one customer host
per AS, a traffic generator, and a Debian container running OpenCode with
`bird-mcp` configured against all five routers.

```
r1 (AS64512) ── r2 (AS64513) ── r3 (AS64514) ── r4 (AS64515) ── r5 (AS64516)
 │                │                │                │                │
h1               h2               h3               h4               h5
sim
```

Each router originates its customer prefix (10.10.N.0/24), exports static routes
over eBGP and installs BGP routes in its kernel, so pings and HTTP between hosts
really traverse the ring. `sim` sits behind r1 and continuously shows the
reachability matrix plus a short iperf3 sample.

## Prerequisites

- Docker with Compose v2.
- The BIRD checkout in `../../bird` on branch `v3.4` (tag `v3.4.0`). It is used
  for the router image and for `birdc` in the agent image.
- Internet access for the OpenCode install during the first build.

## Start

```sh
cd tests/compose
docker compose up -d --build
docker compose logs -f sim
```

The first build compiles BIRD twice (router image, agent image) and installs
OpenCode, so it takes a few minutes.

Check convergence:

```sh
docker compose exec opencode bird-mcp --check
docker compose exec opencode birdc -s /run/bird/r1/bird.ctl show protocols
docker compose exec opencode opencode mcp list   # ✓ bird connected
```

Expected: all routers `up`, both BGP sessions `Established`, `sim` reports
`ping=up` and `http=200` for every host.

## OpenCode

```sh
# log in to a provider once; the volume keeps the credentials
docker compose exec opencode opencode auth login

# then start the TUI in the project directory
docker compose exec -it opencode opencode
```

`/root/.config/opencode/opencode.json` defines one MCP server named `bird`
pointing at `/opt/bird-mcp/routers.toml`, which contains all five routers. The
workdir has `AGENTS.md` with the topology and suggested exercises. Provider keys
can also be passed through `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` or
`OPENROUTER_API_KEY` (see `docker-compose.yml`).

## Manual tests

```sh
# route tables and BGP state
docker compose exec opencode birdc -s /run/bird/r3/bird.ctl show route

# from a host: reach another AS through the ring
docker compose exec h1 ping -c3 10.10.4.3
docker compose exec h1 curl -s http://10.10.5.3/

# through the MCP server (stdio), no model needed
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"shell","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"bird_status","arguments":{"router":"r3"}}}' |
  (docker compose exec -T opencode bird-mcp | tee /dev/stderr) >/dev/null
```

### Blackhole exercise

Add a blackhole for AS64514 on r1 through the MCP tool, or with the CLI:

```sh
docker compose exec opencode bird-mcp   # with an MCP client
# bird_routes_edit {"router":"r1","action":"add","prefix":"10.10.3.0/24"}
```

The route is written to `state/r1/bird-mcp.conf`, checked, applied, and exported
to the other routers. `docker compose logs -f sim` shows 10.10.3.3 going from
`ping=up` to `ping=DOWN` within a few seconds. Remove it again with
`{"action":"remove","prefix":"10.10.3.0/24"}`.

`state/rN/bird-mcp.conf` is the file the agent and router share; the router
includes it as `/etc/bird/state/bird-mcp.conf`.

## Stop

```sh
docker compose down -v
rm -rf state
```

## Notes

- This is a lab, not a deployment guide. All routers are write-enabled, routes
  are not protected by filters, and there is no authentication between the
  control sockets and the agent container.
- The router images need `NET_ADMIN` (kernel protocol, forwarding) and the hosts
  need it to point their default route at the router.
- Docker bridge names and subnets are fixed; if they collide with something on
  your machine, change the `networks` section.
