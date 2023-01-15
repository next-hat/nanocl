-- Your SQL goes here
CREATE TABLE "resource_configs" (
  "key" UUID NOT NULL UNIQUE PRIMARY KEY,
  "resource_key" VARCHAR NOT NULL,
  "config" JSON NOT NULL
);
