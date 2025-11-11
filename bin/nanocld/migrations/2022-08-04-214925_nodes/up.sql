-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "nodes" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "endpoint" VARCHAR NOT NULL,
  "version" VARCHAR NOT NULL,
  "metadata" JSONB
);

CREATE INDEX "nodes_name_idx" ON "nodes" ("name");
CREATE INDEX "nodes_created_at_idx" ON "nodes" ("created_at");
CREATE INDEX "nodes_endpoint_idx" ON "nodes" ("endpoint");
CREATE INDEX "nodes_version_idx" ON "nodes" ("version");
CREATE INDEX "nodes_metadata_idx" ON "nodes" USING GIN ("metadata");
