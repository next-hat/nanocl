-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "process_statuses" (
  "key" VARCHAR NOT NULL PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "current" VARCHAR NOT NULL,
  "previous" VARCHAR NOT NULL,
  "wanted" VARCHAR NOT NULL
);
