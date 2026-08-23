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
