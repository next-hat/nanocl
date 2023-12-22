-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "specs" (
  "key" UUID NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind_name" VARCHAR NOT NULL,
  "kind_key" VARCHAR NOT NULL,
  "version" VARCHAR NOT NULL,
  "data" JSON NOT NULL,
  "metadata" JSON
);
