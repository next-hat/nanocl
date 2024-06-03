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
  "actor" JSONB,
  "related" JSONB,
  "metadata" JSONB
) WITH (ttl_expiration_expression = 'expires_at');

CREATE INDEX "events_key_idx" ON "events" ("key");
CREATE INDEX "events_created_at_idx" ON "events" ("created_at");
CREATE INDEX "events_expires_at_idx" ON "events" ("expires_at");
CREATE INDEX "events_reporting_node_idx" ON "events" ("reporting_node");
CREATE INDEX "events_reporting_controller_idx" ON "events" ("reporting_controller");
CREATE INDEX "events_kind_idx" ON "events" ("kind");
CREATE INDEX "events_action_idx" ON "events" ("action");
CREATE INDEX "events_reason_idx" ON "events" ("reason");
CREATE INDEX "events_note_idx" ON "events" ("note");
CREATE INDEX "events_actor_idx" ON "events" USING GIN ("actor");
CREATE INDEX "events_related_idx" ON "events" USING GIN ("related");
CREATE INDEX "events_metadata_idx" ON "events" USING GIN ("metadata");
