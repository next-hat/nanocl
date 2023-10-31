FROM ubuntu:latest
RUN apt-get update
RUN apt-get install -y wget certbot python3-certbot-nginx
RUN wget https://github.com/nxthat/nanocl/releases/download/nanocl-0.10.0-nightly/nanocl_0.10.0_amd64.deb
RUN dpkg -i nanocl_0.10.0_amd64.deb

COPY ./deploy.sh /deploy.sh

CMD ["/bin/sh", "-c", "certbot -v certonly --dry-run --nginx --agree-tos --email email@email.com -d plex.altair.wtf"]
