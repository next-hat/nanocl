-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "specs" (
  "key" UUID NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind_name" VARCHAR NOT NULL,
  "kind_key" VARCHAR NOT NULL,
  "version" VARCHAR NOT NULL,
  "data" JSONB NOT NULL,
  "metadata" JSONB
);

CREATE INDEX "specs_key_idx" ON "specs" ("key");
CREATE INDEX "specs_created_at_idx" ON "specs" ("created_at");
CREATE INDEX "specs_kind_name_idx" ON "specs" ("kind_name");
CREATE INDEX "specs_kind_key_idx" ON "specs" ("kind_key");
CREATE INDEX "specs_version_idx" ON "specs" ("version");
CREATE INDEX "specs_data_idx" ON "specs" USING GIN ("data");
CREATE INDEX "specs_metadata_idx" ON "specs" USING GIN ("metadata");
