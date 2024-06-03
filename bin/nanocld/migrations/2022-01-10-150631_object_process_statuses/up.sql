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

CREATE INDEX "object_process_statuses_key_idx" ON "object_process_statuses" ("key");
CREATE INDEX "object_process_statuses_created_at_idx" ON "object_process_statuses" ("created_at");
CREATE INDEX "object_process_statuses_updated_at_idx" ON "object_process_statuses" ("updated_at");
CREATE INDEX "object_process_statuses_wanted_idx" ON "object_process_statuses" ("wanted");
CREATE INDEX "object_process_statuses_prev_wanted_idx" ON "object_process_statuses" ("prev_wanted");
CREATE INDEX "object_process_statuses_actual_idx" ON "object_process_statuses" ("actual");
CREATE INDEX "object_process_statuses_prev_actual_idx" ON "object_process_statuses" ("prev_actual");
