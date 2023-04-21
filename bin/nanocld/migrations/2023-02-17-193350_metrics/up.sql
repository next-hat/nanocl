-- Your SQL goes here
CREATE TABLE IF NOT EXISTS metrics (
  "key" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "expire_at" TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '4 month',
  "node_name" TEXT NOT NULL,
  "kind" TEXT NOT NULL,
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
  "node_name" TEXT NOT NULL,
  "uri" TEXT NOT NULL,
  "host" TEXT NOT NULL,
  "remote_addr" TEXT NOT NULL,
  "realip_remote_addr" TEXT NOT NULL,
  "server_protocol" TEXT NOT NULL,
  "request_method" TEXT NOT NULL,
  "proxy_host" TEXT,
  "upstream_addr" TEXT,
  "query_string" TEXT,
  "request_body" TEXT,
  "content_type" TEXT,
  "http_user_agent" TEXT,
  "http_referrer" TEXT,
  "http_accept_language" TEXT
);
