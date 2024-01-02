-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "events" (
  "key" UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "expires_at" TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '4 month',
  "reporting_node" VARCHAR NOT NULL,
  "reporting_controller" VARCHAR NOT NULL,
  "kind" VARCHAR NOT NULL,
  "action" VARCHAR NOT NULL,
  "reason" VARCHAR NOT NULL,
  "note" VARCHAR,
  "actor" JSON,
  "related" JSON,
  "metadata" JSON
) WITH (ttl_expiration_expression = 'expires_at');
