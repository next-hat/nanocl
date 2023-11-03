FROM alpine/openssl:latest

RUN mkdir /work

WORKDIR /work

COPY ssl.sh .

ENTRYPOINT ["sh", "ssl.sh"]
