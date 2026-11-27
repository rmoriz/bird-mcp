# BIRD image for the compose lab. Build context is the repository root so the
# Dockerfile can copy the bird/ checkout; tests/docker/Dockerfile uses bird/ as
# its context for the fixture capture instead.

FROM debian:bookworm-slim AS build
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
    iproute2 iputils-ping libreadline8 tini \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/ /
RUN mkdir -p /run/bird /etc/bird
ENTRYPOINT ["/usr/local/sbin/bird"]
CMD ["-f", "-c", "/etc/bird/bird.conf", "-s", "/run/bird/bird.ctl"]
