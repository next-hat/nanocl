-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "object_process_statuses" (
  "key" VARCHAR NOT NULL PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "wanted" VARCHAR NOT NULL,
  "prev_wanted" VARCHAR NOT NULL,
  "actual" VARCHAR NOT NULL,
  "prev_actual" VARCHAR NOT NULL
);
