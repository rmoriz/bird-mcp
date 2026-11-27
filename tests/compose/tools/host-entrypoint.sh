#!/bin/sh
# Customer host: default route points at the local BIRD router, serves HTTP and
# runs an iperf3 server for the traffic generator.
set -eu

ip route replace default via "$GATEWAY"

mkdir -p /srv
printf '<h1>%s</h1>\n<p>IP: %s</p>\n' \
    "$(hostname)" \
    "$(ip -4 -o addr show dev eth0 | awk '{print $4}')" >/srv/index.html

iperf3 -s -D

exec httpd -f -p 80 -h /srv
