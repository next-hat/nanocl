FROM alpine:latest

COPY ./mock /mock

CMD ["sh", "-c",  "ls /mock && ls /certs && cp /mock/* /certs/"]
