-- Your SQL goes here
CREATE TABLE
  IF NOT EXISTS "distributed_mutexes" (
    "key" UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    "action" TEXT NOT NULL,
    "resource_kind" TEXT NOT NULL,
    "resource_key" TEXT NOT NULL,
    "node_key" TEXT NOT NULL,
    "acquired_by" TEXT NOT NULL,
    "acquired_at" TIMESTAMPTZ NOT NULL DEFAULT now (),
    "expires_at" TIMESTAMPTZ NOT NULL DEFAULT (now () + INTERVAL '5 minutes'),
    UNIQUE ("resource_kind", "resource_key")
  );

CREATE INDEX IF NOT EXISTS idx_distributed_mutexes_resource ON distributed_mutexes (resource_kind, resource_key);

CREATE INDEX IF NOT EXISTS idx_distributed_mutexes_node ON distributed_mutexes (node_key);

CREATE INDEX IF NOT EXISTS idx_distributed_mutexes_acquired ON distributed_mutexes (acquired_by);

CREATE INDEX IF NOT EXISTS idx_distributed_mutexes_acquired_at ON distributed_mutexes (acquired_at);

CREATE INDEX IF NOT EXISTS idx_distributed_mutexes_expires_at ON distributed_mutexes (expires_at);
