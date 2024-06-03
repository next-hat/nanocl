-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "metrics" (
  "key" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "expires_at" TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '4 month',
  "node_name" VARCHAR NOT NULL,
  "kind" VARCHAR NOT NULL,
  "data" JSONB NOT NULL,
  "note" VARCHAR
) WITH (ttl_expiration_expression = 'expires_at');

CREATE INDEX "metrics_key_idx" ON "metrics" ("key");
CREATE INDEX "metrics_created_at_idx" ON "metrics" ("created_at");
CREATE INDEX "metrics_expires_at_idx" ON "metrics" ("expires_at");
CREATE INDEX "metrics_node_name_idx" ON "metrics" ("node_name");
CREATE INDEX "metrics_kind_idx" ON "metrics" ("kind");
CREATE INDEX "metrics_data_idx" ON "metrics" USING GIN ("data");
