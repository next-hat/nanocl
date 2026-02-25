-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "vms" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "name" VARCHAR NOT NULL,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "namespace_name" VARCHAR NOT NULL REFERENCES namespaces("name"),
  "status_key" VARCHAR NOT NULL REFERENCES object_process_statuses("key"),
  "spec_key" UUID NOT NULL REFERENCES specs("key")
);

CREATE INDEX "vms_key_idx" ON "vms" ("key");
CREATE INDEX "vms_name_idx" ON "vms" ("name");
CREATE INDEX "vms_created_at_idx" ON "vms" ("created_at");
CREATE INDEX "vms_namespace_name_idx" ON "vms" ("namespace_name");
CREATE INDEX "vms_status_key_idx" ON "vms" ("status_key");
CREATE INDEX "vms_spec_key_idx" ON "vms" ("spec_key");
