#!/bin/sh
# Traffic generator behind AS64512. Reachability depends entirely on what the
# five BIRD routers announce and install, so blackholing a prefix through the
# MCP tools shows up here within a few seconds.
set -u

ip route replace default via "$GATEWAY"

targets="10.10.2.3 10.10.3.3 10.10.4.3 10.10.5.3"
cycle=0

while true; do
    cycle=$((cycle + 1))
    for target in $targets; do
        if ping -c1 -W1 "$target" >/dev/null 2>&1; then
            ping_state=up
        else
            ping_state=DOWN
        fi
        code=$(curl -s -m2 -o /dev/null -w '%{http_code}' "http://$target/" 2>/dev/null || true)
        printf '%s dst=%-11s ping=%-4s http=%s\n' \
            "$(date +%H:%M:%S)" "$target" "$ping_state" "${code:-000}"
    done

    # Every minute: a short throughput sample to AS64515.
    if [ $((cycle % 12)) -eq 0 ]; then
        printf '%s iperf3 10.10.1.4 -> 10.10.4.3: ' "$(date +%H:%M:%S)"
        result=$(iperf3 -c 10.10.4.3 -t 3 2>/dev/null | grep receiver | tail -1)
        echo "${result:-failed}"
    fi

    sleep 5
done
