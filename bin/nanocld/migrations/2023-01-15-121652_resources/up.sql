-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "resource_kinds" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "spec_key" UUID NOT NULL REFERENCES specs("key")
);

CREATE INDEX "resource_kinds_name_idx" ON "resource_kinds" ("name");
CREATE INDEX "resource_kinds_created_at_idx" ON "resource_kinds" ("created_at");
CREATE INDEX "resource_kinds_spec_key_idx" ON "resource_kinds" ("spec_key");

CREATE TABLE IF NOT EXISTS "resources" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind" VARCHAR NOT NULL,
  "spec_key" UUID NOT NULL REFERENCES specs("key")
);

CREATE INDEX "resources_key_idx" ON "resources" ("key");
CREATE INDEX "resources_created_at_idx" ON "resources" ("created_at");
CREATE INDEX "resources_kind_idx" ON "resources" ("kind");
CREATE INDEX "resources_spec_key_idx" ON "resources" ("spec_key");
