-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "cargo_configs" (
  "key" UUID NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "cargo_key" VARCHAR NOT NULL,
  "version" VARCHAR NOT NULL,
  "config" JSON NOT NULL
);

CREATE TABLE IF NOT EXISTS "cargoes" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "name" VARCHAR NOT NULL,
  "config_key" UUID NOT NULL REFERENCES cargo_configs("key"),
  "namespace_name" VARCHAR NOT NULL REFERENCES namespaces("name")
);
