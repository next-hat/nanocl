-- Your SQL goes here
CREATE TYPE "resource_kind" AS ENUM ('proxy_rule');

CREATE TABLE "resources" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "kind" resource_kind NOT NULL,
  "config_key" UUID NOT NULL references resource_configs("key")
)
