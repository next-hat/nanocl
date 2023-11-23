-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "resource_specs" (
  "key" UUID NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "resource_key" VARCHAR NOT NULL,
  "version" VARCHAR NOT NULL,
  "data" JSON NOT NULL,
  "metadata" JSON
);

CREATE TABLE IF NOT EXISTS "resource_kinds" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS "resource_kind_versions" (
  "resource_kind_name" VARCHAR NOT NULL REFERENCES resource_kinds("name"),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "version" VARCHAR NOT NULL,
  "schema" JSON,
  "url" VARCHAR,
  PRIMARY KEY ("resource_kind_name", "version")
);

CREATE TABLE IF NOT EXISTS "resources" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind" VARCHAR NOT NULL,
  "config_key" UUID NOT NULL REFERENCES resource_specs("key")
);
