-- Your SQL goes here
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
