-- Your SQL goes here-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "processes" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "name" VARCHAR NOT NULL,
  "kind" VARCHAR NOT NULL,
  "data" JSON NOT NULL,
  "node_key" VARCHAR NOT NULL,
  "kind_key" VARCHAR NOT NULL
);
