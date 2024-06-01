CREATE TABLE IF NOT EXISTS "cargoes" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "name" VARCHAR NOT NULL,
  "spec_key" UUID NOT NULL REFERENCES specs("key"),
  "status_key" VARCHAR NOT NULL REFERENCES object_process_statuses("key"),
  "namespace_name" VARCHAR NOT NULL REFERENCES namespaces("name")
);

CREATE INDEX "cargoes_key_idx" ON "cargoes" ("key");
CREATE INDEX "cargoes_created_at_idx" ON "cargoes" ("created_at");
CREATE INDEX "cargoes_name_idx" ON "cargoes" ("name");
CREATE INDEX "cargoes_spec_key_idx" ON "cargoes" ("spec_key");
CREATE INDEX "cargoes_status_key_idx" ON "cargoes" ("status_key");
CREATE INDEX "cargoes_namespace_name_idx" ON "cargoes" ("namespace_name");
