CREATE TABLE operations (
  key UUID PRIMARY KEY,
  incarnation_key UUID NOT NULL REFERENCES resource_incarnations (key) ON DELETE RESTRICT,
  resource_kind TEXT NOT NULL,
  resource_key TEXT NOT NULL,
  action TEXT NOT NULL,
  spec_key UUID NOT NULL REFERENCES specs (key) ON DELETE RESTRICT,
  node_name TEXT NOT NULL,
  owner_boot_id TEXT NOT NULL,
  lock_device BIGINT NOT NULL,
  lock_inode BIGINT NOT NULL,
  accepted_at TIMESTAMPTZ NOT NULL,
  completed_at TIMESTAMPTZ,
  outcome TEXT CHECK (outcome IN ('completed', 'failed', 'interrupted')),
  diagnostic JSONB,
  UNIQUE (incarnation_key, key),
  CHECK ((outcome IS NULL) = (completed_at IS NULL))
);
