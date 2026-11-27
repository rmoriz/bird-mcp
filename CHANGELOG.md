# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `bird_fleet` runs a read-only check (status, protocols, routes, interfaces) on
  many routers in parallel with bounded concurrency, returning per-router
  results and `elapsed_ms`. `bird_routers` probes run in parallel too.
- The compose lab can reuse the host's OpenCode Go login: `OPENCODE_API_KEY`
  from `.env`, `use-host-login.sh` (extracts the host key into `.env`), or a shared
  OpenCode data directory.
- `tests/compose/`: a five-AS manual lab (ring of five BIRD routers with eBGP,
  one customer host per AS, a traffic generator doing ping, HTTP and iperf3) and
  a Debian OpenCode container with `bird-mcp` wired to all five routers over
  shared control sockets.
- `bird_routes_edit`: add, remove and list static routes (blackhole, unreachable,
  prohibit or via gateway/interface) in a managed include file, with
  `configure check` before apply, automatic restore on failure and `dry_run` /
  `apply=false` staging. Routes are rendered into the static protocols
  `bird_mcp_static4` and `bird_mcp_static6`; new router settings
  `routes_file`/`routes_preference`.
- `AGENTS.md` for coding agents and an AI usage section in the README.
- GitHub Actions CI (format, clippy, tests) and a release workflow that builds
  native binaries for macOS arm64/Intel and Linux arm64/x86_64 on every push to
  `main`. The rolling `latest` release is a regular release with stable asset
  names, so the `/releases/latest/download/...` links always work; `v*` tags
  create fixed releases with generated notes.

### Fixed
- Retry `exec` on `ETXTBSY`, which Linux returns when a just-written script is
  still open for writing in a forked child (hit by the transport tests in CI).
- `bird_routes` treats `Network not found` for a specific prefix as an empty
  result instead of a command failure.

## [0.1.0] - 2026-11-27

### Added
- MCP server for the BIRD Internet Routing Daemon over stdio, with tools for
  router status, protocols (including detailed BGP sessions), routes, interfaces,
  configuration reading, configuration check/apply/undo/confirm, protocol
  enable/disable/restart/reload, and documentation lookup.
- Local `birdc` and ssh transports. Remote commands are quoted so filters and
  patterns survive the remote shell; commands time out and report stderr.
- Parsers for BIRD 3.4.0 output: status, protocol tables and detail blocks,
  routes (including inherited prefixes, next hops, preferences and verbose
  attributes), route counts, interfaces and interface summaries, symbols and
  configuration checks.
- Read-only routers by default. Writes require `read_only = false`; `bird_show`
  only accepts commands starting with `show`, and all write arguments are
  validated.
- Documentation tools that search a BIRD source checkout (`bird.sgml`,
  `bird.conf.example`, migration notes) by text or label.
- Test suite with fixtures recorded from a two-router BIRD 3.4.0 docker lab,
  an MCP stdio end-to-end test, and an ignored live test that also exercises the
  write path.
