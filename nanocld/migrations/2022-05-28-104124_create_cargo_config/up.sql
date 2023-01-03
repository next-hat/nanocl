-- Your SQL goes here
CREATE TABLE "cargo_configs" (
  "key" UUID NOT NULL UNIQUE PRIMARY KEY,
  "cargo_key" VARCHAR NOT NULL,
  "config" JSON NOT NULL
);
