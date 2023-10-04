-- Your SQL goes here
-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "secrets" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind" VARCHAR NOT NULL,
  "immutable" BOOLEAN NOT NULL DEFAULT FALSE,
  "data" JSON NOT NULL,
  "metadata" JSON
);
