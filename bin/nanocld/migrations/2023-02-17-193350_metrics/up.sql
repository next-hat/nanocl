-- Your SQL goes here
CREATE TABLE IF NOT EXISTS metrics (
  "key" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "expire_at" TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '4 month',
  "node_name" VARCHAR NOT NULL,
  "kind" VARCHAR NOT NULL,
  "data" JSON NOT NULL
) WITH (ttl_expiration_expression = 'expire_at');
