CREATE TABLE IF NOT EXISTS "networks" (
  "key" VARCHAR NOT NULL PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "name" VARCHAR NOT NULL,
  "node_name" VARCHAR NOT NULL REFERENCES "nodes"("name") ON DELETE CASCADE,
  "data" JSONB NOT NULL,
  "metadata" JSONB,
  UNIQUE ("node_name", "name")
);

CREATE INDEX "networks_created_at_idx" ON "networks" ("created_at");
CREATE INDEX "networks_updated_at_idx" ON "networks" ("updated_at");
CREATE INDEX "networks_name_idx" ON "networks" ("name");
CREATE INDEX "networks_node_name_idx" ON "networks" ("node_name");
CREATE INDEX "networks_data_idx" ON "networks" USING GIN ("data");
CREATE INDEX "networks_metadata_idx" ON "networks" USING GIN ("metadata");

CREATE TABLE IF NOT EXISTS "cargo_replicas" (
  "key" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "cargo_key" VARCHAR NOT NULL REFERENCES "cargoes"("key") ON DELETE CASCADE,
  "ordinal" INT4 NOT NULL,
  "node_name" VARCHAR REFERENCES "nodes"("name") ON DELETE SET NULL,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT "cargo_replicas_cargo_key_ordinal_unique"
    UNIQUE ("cargo_key", "ordinal"),
  CONSTRAINT "cargo_replicas_ordinal_nonnegative"
    CHECK ("ordinal" >= 0)
);

CREATE INDEX "cargo_replicas_cargo_key_idx"
  ON "cargo_replicas" ("cargo_key");
CREATE INDEX "cargo_replicas_node_name_idx"
  ON "cargo_replicas" ("node_name");

CREATE TABLE IF NOT EXISTS "cargo_replica_processes" (
  "key" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "replica_key" UUID NOT NULL
    REFERENCES "cargo_replicas"("key") ON DELETE CASCADE,
  "process_key" VARCHAR
    REFERENCES "processes"("key") ON DELETE SET NULL,
  "container_name" VARCHAR NOT NULL,
  "role" VARCHAR NOT NULL,
  "essential" BOOLEAN NOT NULL,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT "cargo_replica_processes_identity_unique"
    UNIQUE ("replica_key", "role", "container_name"),
  CONSTRAINT "cargo_replica_processes_role_valid"
    CHECK ("role" IN ('sandbox', 'init', 'app')),
  CONSTRAINT "cargo_replica_processes_role_essential"
    CHECK ("role" = 'app' OR "essential"),
  CONSTRAINT "cargo_replica_processes_sandbox_name"
    CHECK (
      ("role" = 'sandbox' AND "container_name" = '_sandbox')
      OR
      ("role" <> 'sandbox' AND "container_name" <> '_sandbox')
    )
);

CREATE INDEX "cargo_replica_processes_replica_key_idx"
  ON "cargo_replica_processes" ("replica_key");
CREATE UNIQUE INDEX "cargo_replica_processes_process_key_idx"
  ON "cargo_replica_processes" ("process_key")
  WHERE "process_key" IS NOT NULL;
CREATE INDEX "cargo_replica_processes_replica_container_name_idx"
  ON "cargo_replica_processes" ("replica_key", "container_name");
