FROM nginx:1.24.0-bullseye

USER root

ENV TZ="Europe/Paris"

RUN apt-get update -y
RUN apt-get install -y nginx-extras certbot python3-certbot-nginx

EXPOSE 80/tcp 443/tcp

RUN mkdir -p /var/lib/nginx/cache/public
RUN mkdir -p /var/lib/nginx/cache/private
RUN mkdir -p /tmp/nginx/body
RUN mkdir -p /var/log/nginx/access
RUN touch /var/log/nginx/access/http.log
RUN touch /var/log/nginx/access/stream.log

COPY ./bin/nproxy/html /html
COPY ./bin/nproxy/entrypoint.sh /entrypoint.sh
COPY ./bin/nproxy/nginx.conf /etc/nginx/nginx.conf

RUN chmod +x /entrypoint.sh

LABEL org.opencontainers.image.source https://github.com/nxthat/nanocl
LABEL org.opencontainers.image.description Nanocl Proxy

ENTRYPOINT ["/entrypoint.sh"]
