FROM ubuntu:noble-20240212

USER root

RUN apt-get update && \
  apt-get install -y nginx nginx-extras \
  && rm -rf /var/lib/apt/lists/* \
  && rm -rf /tmp/* \
  && rm -rf /var/log/* \
  && rm -rf /var/tmp/*

EXPOSE 80/tcp 443/tcp

COPY ./bin/nproxy/html /html
COPY ./bin/nproxy/entrypoint.sh /entrypoint.sh

RUN chmod +x /entrypoint.sh

LABEL org.opencontainers.image.source https://github.com/next-hat/nanocl
LABEL org.opencontainers.image.description Nanocl Proxy

ENTRYPOINT ["/entrypoint.sh"]
