-- Your SQL goes here
CREATE TABLE IF NOT EXISTS metrics (
  "key" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "expire_at" TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '4 month',
  "node_name" VARCHAR NOT NULL,
  "kind" VARCHAR NOT NULL,
  "data" JSON NOT NULL
) WITH (ttl_expiration_expression = 'expire_at');

CREATE TABLE IF NOT EXISTS http_metrics (
  "key" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "expire_at" TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '4 month',
  "date_gmt" TIMESTAMPTZ NOT NULL,
  "status" INT NOT NULL,
  "bytes_sent" INT NOT NULL,
  "content_length" INT NOT NULL,
  "body_bytes_sent" INT NOT NULL,
  "request_time" FLOAT8 NOT NULL,
  "node_name" VARCHAR NOT NULL,
  "uri" VARCHAR NOT NULL,
  "host" VARCHAR NOT NULL,
  "remote_addr" VARCHAR NOT NULL,
  "realip_remote_addr" VARCHAR NOT NULL,
  "server_protocol" VARCHAR NOT NULL,
  "request_method" VARCHAR NOT NULL,
  "proxy_host" VARCHAR,
  "upstream_addr" VARCHAR,
  "query_string" VARCHAR,
  "request_body" VARCHAR,
  "content_type" VARCHAR,
  "http_user_agent" VARCHAR,
  "http_referrer" VARCHAR,
  "http_accept_language" VARCHAR
) WITH (ttl_expiration_expression = 'expire_at');

CREATE TABLE IF NOT EXISTS stream_metrics (
  "key" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "expire_at" TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '4 month',
  "date_gmt" TIMESTAMPTZ NOT NULL,
  "remote_addr" VARCHAR NOT NULL,
  "upstream_addr" VARCHAR NOT NULL,
  "protocol" VARCHAR,
  "status" INT NOT NULL,
  "session_time" VARCHAR NOT NULL,
  "bytes_sent" INT NOT NULL,
  "bytes_received" INT NOT NULL,
  "upstream_bytes_sent" INT NOT NULL,
  "upstream_bytes_received" INT NOT NULL,
  "upstream_connect_time" VARCHAR NOT NULL,
  "node_name" VARCHAR NOT NULL
) WITH (ttl_expiration_expression = 'expire_at');
