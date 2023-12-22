-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "resource_specs" (
  "key" UUID NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "resource_key" VARCHAR NOT NULL,
  "version" VARCHAR NOT NULL,
  "data" JSON NOT NULL,
  "metadata" JSON
);

CREATE TABLE IF NOT EXISTS "resource_kind_versions" (
  "key" UUID NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind_name" VARCHAR NOT NULL,
  "kind_key" VARCHAR NOT NULL,
  "version" VARCHAR NOT NULL,
  "data" JSON NOT NULL,
  "metadata" JSON
);

CREATE TABLE IF NOT EXISTS "resource_kinds" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "version_key" UUID NOT NULL REFERENCES resource_kind_versions("key"),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS "resources" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind" VARCHAR NOT NULL,
  "spec_key" UUID NOT NULL REFERENCES resource_specs("key")
);
