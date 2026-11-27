# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
