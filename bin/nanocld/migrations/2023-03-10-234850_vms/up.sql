-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "vm_images" (
  "name" VARCHAR NOT NULL PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "kind" VARCHAR NOT NULL,
  "path" VARCHAR NOT NULL,
  "format" VARCHAR NOT NULL,
  "size_actual" BIGINT NOT NULL,
  "size_virtual" BIGINT NOT NULL,
  "parent" VARCHAR REFERENCES vm_images("name")
);

CREATE TABLE IF NOT EXISTS "vms" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "name" VARCHAR NOT NULL,
  "spec_key" UUID NOT NULL REFERENCES specs("key"),
  "namespace_name" VARCHAR NOT NULL REFERENCES namespaces("name")
);
