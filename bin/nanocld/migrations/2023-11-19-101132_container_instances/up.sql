-- Your SQL goes here-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "container_instances" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "name" VARCHAR NOT NULL,
  "kind" VARCHAR NOT NULL,
  "data" JSON NOT NULL,
  "node_id" VARCHAR NOT NULL,
  "kind_id" VARCHAR NOT NULL
);
