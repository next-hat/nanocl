-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "jobs" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "status_key" VARCHAR NOT NULL REFERENCES object_process_statuses("key"),
  "spec_key" UUID NOT NULL REFERENCES specs("key") ON DELETE RESTRICT
);

CREATE INDEX "jobs_key_idx" ON "jobs" ("key");
CREATE INDEX "jobs_created_at_idx" ON "jobs" ("created_at");
CREATE INDEX "jobs_updated_at_idx" ON "jobs" ("updated_at");
CREATE INDEX "jobs_status_key_idx" ON "jobs" ("status_key");
