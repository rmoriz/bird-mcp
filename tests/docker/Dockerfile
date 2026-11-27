FROM debian:bookworm-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential autoconf autoconf-archive flex bison \
    libreadline-dev libncurses-dev pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# Build context is the BIRD source checkout.
COPY . /src
WORKDIR /src
RUN autoreconf -i \
    && ./configure --prefix=/usr/local --sysconfdir=/etc/bird \
    && make -j"$(nproc)" \
    && make install DESTDIR=/out

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    libreadline8 iproute2 iputils-ping tini \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/ /
RUN mkdir -p /run/bird /etc/bird
ENTRYPOINT ["/usr/local/sbin/bird"]
CMD ["-f", "-c", "/etc/bird/bird.conf", "-s", "/run/bird/bird.ctl"]
