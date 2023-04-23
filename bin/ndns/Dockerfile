FROM alpine:3.17.2

RUN apk -U upgrade
RUN apk add dnsmasq

EXPOSE 53/tcp 53/udp

COPY ./bin/ndns/dnsmasq.conf /dnsmasq.conf

COPY ./bin/ndns/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]

