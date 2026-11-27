#!/bin/sh
# Rebuilds the two-router BIRD lab and recaptures tests/fixtures/.
#
# Needs docker and the BIRD source checkout in <repo>/bird (pinned to v3.4.0).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
fixtures="$root/tests/fixtures"

if ! docker image inspect bird:3.4.0 >/dev/null 2>&1; then
  docker build -f "$here/Dockerfile" -t bird:3.4.0 "$root/bird"
fi

docker network inspect birdnet >/dev/null 2>&1 \
  || docker network create --subnet 172.30.0.0/24 birdnet >/dev/null
docker rm -f bird-r1 bird-r2 >/dev/null 2>&1 || true

docker run -d --name bird-r1 --network birdnet --ip 172.30.0.11 \
  -v "$here/r1.conf:/etc/bird/bird.conf:ro" bird:3.4.0 >/dev/null
docker run -d --name bird-r2 --network birdnet --ip 172.30.0.12 \
  -v "$here/r2.conf:/etc/bird/bird.conf:ro" bird:3.4.0 >/dev/null

sleep 3
docker cp "$here/bad.conf" bird-r1:/etc/bird/bad.conf >/dev/null

b() {
  docker exec bird-r1 birdc -s /run/bird/bird.ctl "$@"
}

mkdir -p "$fixtures"
b show status >"$fixtures/status.txt"
b show protocols >"$fixtures/protocols.txt"
b show protocols all >"$fixtures/protocols-all.txt"
b show protocols all peer2 >"$fixtures/protocol-peer2.txt"
b show route >"$fixtures/route.txt"
b show route all >"$fixtures/route-all.txt"
b show route count >"$fixtures/route-count.txt"
b show route 192.0.2.0/24 >"$fixtures/route-prefix.txt"
b show route for 203.0.113.5 >"$fixtures/route-for.txt"
b show interfaces >"$fixtures/interfaces.txt"
b show interfaces summary >"$fixtures/interfaces-summary.txt"
b show symbols | head -40 >"$fixtures/symbols.txt"

b configure check >"$fixtures/config-check.txt" 2>&1 || true
b configure check '"/etc/bird/bad.conf"' >"$fixtures/config-check-error.txt" 2>&1 || true
b foo bar >"$fixtures/error-syntax.txt" 2>&1 || true

echo "fixtures written to $fixtures"
