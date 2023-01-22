-- Your SQL goes here

CREATE TABLE "resources" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "kind" VARCHAR NOT NULL,
  "config_key" UUID NOT NULL references resource_configs("key")
)
