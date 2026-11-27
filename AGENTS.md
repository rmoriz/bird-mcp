# AGENTS.md

Guidance for coding agents working on this repository.

## What this is

`bird-mcp` is a Model Context Protocol server over stdio for the BIRD Internet
Routing Daemon. It runs `birdc` locally or over ssh, parses BIRD 3.4 CLI output,
and exposes it as JSON tools. Write operations (configure, protocol control,
managed static routes) are gated per router.

- Rust 2024, MSRV 1.88, pinned toolchain in `rust-toolchain.toml` (1.100.0).
- `./bird` is an upstream BIRD checkout (gitignored, branch `v3.4`, tag
  `v3.4.0`). It is used for documentation search and to build the test lab; the
  crate does not link against it.
- Single binary: `src/main.rs`, all logic in the `bird_mcp` library.

## Commands

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test                      # unit + parser + transport + MCP stdio tests
cargo test --test live_docker -- --ignored   # needs the docker lab
./tests/docker/capture.sh       # rebuild the lab and recapture fixtures
cargo run -- --check            # validate router configuration
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy and tests on every push/PR.
The release workflow (`.github/workflows/release.yml`) builds macOS arm64/Intel
and Linux arm64/x86_64 on every push to `main` and refreshes the rolling
`latest` release with stable asset names; `v*` tags create fixed releases.

## Layout

| File | Responsibility |
| --- | --- |
| `src/config.rs` | Router config from env/TOML/JSON, defaults, validation |
| `src/transport.rs` | Local/ssh execution, timeouts, quoting, file read/write/remove |
| `src/parse.rs` | Parsers for `show status/protocols/route/interfaces/symbols` and `configure check` |
| `src/routes.rs` | Managed static-route include file: parse, render, add, remove |
| `src/docs.rs` | Search/read of the BIRD docs in `./bird/doc` |
| `src/server.rs` | MCP tool definitions (`#[tool_router]`), argument validation |
| `tests/fixtures/` | Real BIRD 3.4.0 output used by parser tests |
| `tests/docker/` | Reproducible two-router BIRD lab (not used by CI) |
| `tests/mcp.rs` | End-to-end MCP stdio test with a fake `birdc` |
| `tests/live_docker.rs` | Ignored test against the real lab, including writes |

## Rules

- `unsafe` is forbidden by the crate lint. Do not add it.
- Every write path checks `read_only` first. Keep that gate and keep the
  "read-only" wording in errors; tests assert on it.
- `bird_show` accepts only commands starting with `show`. Any argument that is
  interpolated into a birdc line goes through `validate_*` in `src/server.rs`.
- Local commands pass the whole CLI line as one argument (birdc joins argv);
  ssh commands quote every word with `shell_quote`. Never build a shell command
  from caller-supplied text.
- Parsers must degrade gracefully: unknown lines become warnings or raw detail,
  never panics. Pin behavior with a fixture from `tests/fixtures/`.
- Do not edit fixtures by hand. Run `tests/docker/capture.sh` with the BIRD
  `v3.4.0` checkout and commit the result.
- When adding or renaming a tool, update `tests/mcp.rs` (tool list), the README
  tool table and `CHANGELOG.md` under `[Unreleased]`.
- Commit messages explain why; no `Co-Authored-By` trailers.

## Configuration surface

Keep these in sync with the README:

- Router fields: `name`, `transport`, `host`, `user`, `port`, `birdc`, `socket`,
  `config_file`, `routes_file`, `routes_preference`, `read_only`,
  `timeout_secs`.
- Env: `BIRD_MCP_ROUTERS`, `BIRD_MCP_CONFIG`, `BIRD_MCP_DOCS_DIR`,
  `BIRD_MCP_TIMEOUT`, `BIRD_MCP_BIRDC`, `BIRD_MCP_SOCKET`,
  `BIRD_MCP_CONFIG_FILE`, `BIRD_MCP_ROUTES_FILE`,
  `BIRD_MCP_ROUTES_PREFERENCE`, `BIRD_MCP_SSH_BIN`.

## Adding a tool

1. Add an args struct deriving `Deserialize + JsonSchema` with doc comments for
   every field (they become the tool schema).
2. Add an `async fn bird_<name>(&self, Parameters(args): Parameters<Args>)`
   method inside the `#[tool_router] impl Bird` with a `#[tool(description = ...)]`.
3. Return `Result<CallToolResult, ErrorData>` and build the body with
   `respond(json!({...}))`; map failures through `ToolError`.
4. Add parser code to `src/parse.rs` (or `src/routes.rs`) with unit tests when
   the tool parses BIRD output.
5. Extend `tests/mcp.rs` with a fake-birdc dispatch case and an assertion;
   extend `tests/live_docker.rs` when real BIRD behavior matters.

## AI usage

Substantial parts of this repository were written with AI assistance under human
direction and review. See the AI usage section in `README.md`.
