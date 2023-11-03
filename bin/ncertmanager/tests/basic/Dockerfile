FROM alpine:latest

COPY ./mock /mock

CMD ["sh", "-c",  "ls /mock && ls /certs && cp /mock/cert.crt /certs/$DOMAIN.crt && cp /mock/privkey.key /certs/$DOMAIN.key"]
