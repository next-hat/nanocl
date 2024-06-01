-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "secrets" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind" VARCHAR NOT NULL,
  "immutable" BOOLEAN NOT NULL DEFAULT FALSE,
  "data" JSONB NOT NULL,
  "metadata" JSONB
);

CREATE INDEX "secrets_key_idx" ON "secrets" ("key");
CREATE INDEX "secrets_created_at_idx" ON "secrets" ("created_at");
CREATE INDEX "secrets_updated_at_idx" ON "secrets" ("updated_at");
CREATE INDEX "secrets_kind_idx" ON "secrets" ("kind");
CREATE INDEX "secrets_immutable_idx" ON "secrets" ("immutable");
CREATE INDEX "secrets_data_idx" ON "secrets" USING GIN ("data");
CREATE INDEX "secrets_metadata_idx" ON "secrets" USING GIN ("metadata");
