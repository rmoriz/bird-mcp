FROM alpine:3.20

RUN apk add --no-cache \
    bash \
    bind-tools \
    busybox-extras \
    curl \
    iproute2 \
    iperf3 \
    iputils \
    jq \
    mtr \
    tcpdump \
    && rm -rf /var/cache/apk/*

CMD ["/bin/sh"]
