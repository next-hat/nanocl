FROM haproxy:lts-alpine3.22

USER root

EXPOSE 80/tcp 443/tcp 443/udp

COPY ./bin/nproxy/html /html
COPY ./bin/nproxy/entrypoint.sh /entrypoint.sh

RUN chmod +x /entrypoint.sh

LABEL org.opencontainers.image.source=https://github.com/next-hat/nanocl
LABEL org.opencontainers.image.description="Nanocl Proxy"

ENTRYPOINT ["/entrypoint.sh"]
