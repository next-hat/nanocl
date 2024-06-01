-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "vm_images" (
  "name" VARCHAR NOT NULL PRIMARY KEY,
  "node_name" VARCHAR NOT NULL REFERENCES nodes("name"),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind" VARCHAR NOT NULL,
  "path" VARCHAR NOT NULL,
  "format" VARCHAR NOT NULL,
  "size_actual" BIGINT NOT NULL,
  "size_virtual" BIGINT NOT NULL,
  "parent" VARCHAR REFERENCES vm_images("name")
);

CREATE INDEX "vm_images_name_idx" ON "vm_images" ("name");
CREATE INDEX "vm_images_node_name_idx" ON "vm_images" ("node_name");
CREATE INDEX "vm_images_created_at_idx" ON "vm_images" ("created_at");
CREATE INDEX "vm_images_kind_idx" ON "vm_images" ("kind");
CREATE INDEX "vm_images_path_idx" ON "vm_images" ("path");
CREATE INDEX "vm_images_format_idx" ON "vm_images" ("format");
CREATE INDEX "vm_images_size_actual_idx" ON "vm_images" ("size_actual");
CREATE INDEX "vm_images_size_virtual_idx" ON "vm_images" ("size_virtual");
CREATE INDEX "vm_images_parent_idx" ON "vm_images" ("parent");

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
