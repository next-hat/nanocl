FROM alpine:3.17.2

RUN apk -U upgrade
RUN apk add dnsmasq

EXPOSE 53/tcp 53/udp

COPY ./bin/ndns/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

LABEL org.opencontainers.image.source https://github.com/nxthat/nanocl
LABEL org.opencontainers.image.description Nanocl Dns

ENTRYPOINT ["/entrypoint.sh"]

