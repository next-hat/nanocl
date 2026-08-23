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
