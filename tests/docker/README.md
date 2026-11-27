# BIRD lab for fixtures and live tests

The files here reproduce the lab that `tests/fixtures/` was recorded from and that
`tests/live_docker.rs` talks to.

- `Dockerfile` — builds BIRD from the checkout in `<repo>/bird` and installs it into
  a Debian image. The build context is the BIRD source tree.
- `r1.conf`, `r2.conf` — two BIRD 3.4.0 routers, AS 64512 and 64513, peering over
  a docker bridge (172.30.0.11 / 172.30.0.12). r1 has two routes for 192.0.2.0/24
  (one primary, one not) and receives a BGP route for 203.0.113.0/24.
- `bad.conf` — syntax error used for the failing `configure check` fixture.
- `capture.sh` — builds the image, starts both containers and rewrites
  `tests/fixtures/` from real birdc output.

```sh
tests/docker/capture.sh
```

Both containers stay up afterwards, which is what the ignored live test expects:

```sh
cargo test --test live_docker -- --ignored
```

Requirements: docker, and the BIRD checkout in `<repo>/bird` on branch/tag `v3.4.0`.

To remove the lab again:

```sh
docker rm -f bird-r1 bird-r2
docker network rm birdnet
```
