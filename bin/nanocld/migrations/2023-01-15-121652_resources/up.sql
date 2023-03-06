-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "resource_configs" (
  "key" UUID NOT NULL UNIQUE PRIMARY KEY,
  "resource_key" VARCHAR NOT NULL,
  "version" VARCHAR NOT NULL,
  "data" JSON NOT NULL
);

CREATE TABLE IF NOT EXISTS "resources" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "kind" VARCHAR NOT NULL,
  "config_key" UUID NOT NULL REFERENCES resource_configs("key")
)
