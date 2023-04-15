FROM nginx:1.23.2

ENV TZ="Europe/Paris"
RUN apt-get update -y

RUN apt-get install -y nginx-extras
RUN apt-get install -y certbot python3-certbot-nginx

EXPOSE 80/tcp 443/tcp

# Copy default nginx config
COPY ./nginx.conf /etc/nginx/nginx.conf
# Copy default error pages
COPY ./html /html

COPY ./entrypoint.sh /entrypoint.sh
RUN mkdir -p /var/lib/nginx/cache/public
RUN mkdir -p /var/lib/nginx/cache/private
RUN mkdir -p /tmp/nginx/body
RUN chmod +x /entrypoint.sh

LABEL org.opencontainers.image.source https://github.com/nxthat/nanocl
LABEL org.opencontainers.image.description Nanocl Proxy

ENTRYPOINT ["/entrypoint.sh"]
