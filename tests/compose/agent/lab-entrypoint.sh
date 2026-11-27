#!/bin/sh
# Give the agent container a data path into the lab: r1 is the gateway for
# both the customer prefixes and the link subnets, while the default route
# keeps internet access and the management network keeps DNS.
set -eu

ip route replace 10.10.0.0/16 via 10.10.1.2 2>/dev/null || true
ip route replace 10.0.0.0/16 via 10.10.1.2 2>/dev/null || true

exec /usr/bin/tini -- opencode "$@"
