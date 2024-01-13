-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "jobs" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "status_key" VARCHAR NOT NULL REFERENCES object_process_statuses("key"),
  "data" JSON NOT NULL,
  "metadata" JSON
);
