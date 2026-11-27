# OpenCode container for the BIRD MCP lab.
#
# Build context is the repository root (it needs src/ and the BIRD checkout for
# birdc). OpenCode is installed with the official v2 installer.

FROM rust:1.100-bookworm AS mcp
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim AS bird
RUN apt-get update && apt-get install -y --no-install-recommends \
    autoconf autoconf-archive bison build-essential ca-certificates flex \
    libncurses-dev libreadline-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY bird /src
WORKDIR /src
RUN autoreconf -i \
    && ./configure --prefix=/usr/local --sysconfdir=/etc/bird \
    && make -j"$(nproc)" \
    && make install DESTDIR=/out

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    bash ca-certificates curl git jq less libreadline8 mtr-tiny nano \
    iproute2 iputils-ping netbase tcpdump tini whois \
    && rm -rf /var/lib/apt/lists/*

COPY --from=bird /out/usr/local/sbin/birdc /usr/local/sbin/birdc
COPY --from=mcp /src/target/release/bird-mcp /usr/local/bin/bird-mcp

# Official installer; drops the binary in /root/.opencode/bin.
RUN curl -fsSL https://opencode.ai/v2/install | bash -s -- --no-modify-path \
    && ln -sf /root/.opencode/bin/opencode /usr/local/bin/opencode \
    && opencode --version

RUN mkdir -p /workspace /opt/bird-mcp
WORKDIR /workspace

ENTRYPOINT ["/usr/bin/tini", "--", "opencode"]
