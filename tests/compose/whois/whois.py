#!/usr/bin/env python3
"""Minimal RIPE-style WHOIS server for the lab's static inetnum data."""

import ipaddress
import socketserver

DATA = "/app/data.txt"


def load_objects(path):
    objects = []
    with open(path, encoding="utf-8") as handle:
        for block in handle.read().split("\n\n"):
            block = block.strip("\n")
            if not block.strip():
                continue
            fields = {}
            for line in block.splitlines():
                if line.startswith("%") or ":" not in line:
                    continue
                key, value = line.split(":", 1)
                fields.setdefault(key.strip().lower(), value.strip())
            inetnum = fields.get("inetnum")
            if not inetnum:
                continue
            start_text, end_text = (part.strip() for part in inetnum.split("-", 1))
            objects.append(
                {
                    "raw": block,
                    "start": int(ipaddress.ip_address(start_text)),
                    "end": int(ipaddress.ip_address(end_text)),
                }
            )
    objects.sort(key=lambda item: item["start"])
    return objects


OBJECTS = load_objects(DATA)


def parse_query(query):
    """Drops client flags and returns the lookup value."""
    tokens = query.split()
    cleaned = []
    skip = False
    for token in tokens:
        if skip:
            skip = False
            continue
        if token in ("-T", "-i", "-s", "-h", "-p"):
            skip = True
            continue
        if token.startswith("-"):
            continue
        cleaned.append(token)
    return cleaned[-1] if cleaned else ""


def answer(query):
    lookup = parse_query(query)
    if not lookup:
        return "%ERROR:101: no entries found\n"

    try:
        if "/" in lookup:
            network = ipaddress.ip_network(lookup, strict=False)
            start, end = int(network.network_address), int(network.broadcast_address)
        else:
            address = int(ipaddress.ip_address(lookup))
            start = end = address
    except ValueError:
        return f"%ERROR:101: no entries found\n% Invalid query: {lookup}\n"

    # Prefer the object that contains the query; otherwise list the objects
    # inside a queried range (e.g. 10.10.0.0/16).
    matches = [obj for obj in OBJECTS if obj["start"] <= start and obj["end"] >= end]
    if not matches:
        matches = [obj for obj in OBJECTS if obj["start"] >= start and obj["end"] <= end]

    header = (
        "% This is the BIRD-MCP lab WHOIS server (fictional).\n"
        f"% Query: {lookup}\n\n"
    )
    if not matches:
        return header + "%ERROR:101: no entries found\n"

    return header + "\n\n".join(obj["raw"] for obj in matches) + "\n\n% No more entries.\n"


class Handler(socketserver.StreamRequestHandler):
    def handle(self):
        line = self.rfile.readline(512)
        if not line:
            return
        query = line.decode("utf-8", "replace").strip()
        if not query:
            return
        print(f"query from {self.client_address[0]}: {query}", flush=True)
        self.wfile.write(answer(query).encode("utf-8"))


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


if __name__ == "__main__":
    with Server(("0.0.0.0", 43), Handler) as server:
        print("BIRD-MCP lab WHOIS listening on 0.0.0.0:43", flush=True)
        server.serve_forever()
