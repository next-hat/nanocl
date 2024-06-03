-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "namespaces" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "metadata" JSONB
);

CREATE INDEX "namespaces_name_idx" ON "namespaces" ("name");
CREATE INDEX "namespaces_created_at_idx" ON "namespaces" ("created_at");
CREATE INDEX "namespaces_metadata_idx" ON "namespaces" USING GIN ("metadata");
