-- Your SQL goes here
CREATE TABLE "nginx_logs" (
  "key" UUID NOT NULL PRIMARY KEY,
  "date_gmt" timestamptz NOT NULL,
  "uri" VARCHAR NOT NULL,
  "host" VARCHAR NOT NULL,
  "remote_addr" VARCHAR NOT NULL,
  "realip_remote_addr" VARCHAR NOT NULL,
  "server_protocol" VARCHAR NOT NULL,
  "request_method" VARCHAR NOT NULL,
  "content_length" bigint NOT NULL,
  "status" int NOT NULL,
  "request_time" FLOAT8 NOT NULL,
  "body_bytes_sent" bigint NOT NULL,
  "proxy_host" VARCHAR,
  "upstream_addr" VARCHAR,
  "query_string" VARCHAR,
  "request_body" VARCHAR,
  "content_type" VARCHAR,
  "http_user_agent" VARCHAR,
  "http_referrer" VARCHAR,
  "http_accept_language" VARCHAR
);
