CREATE TABLE vm_operations (
  vm_key TEXT PRIMARY KEY REFERENCES vms (key) ON DELETE CASCADE,
  key UUID NOT NULL UNIQUE,
  spec_key UUID NOT NULL,
  node_name TEXT NOT NULL,
  phase TEXT NOT NULL CHECK (phase IN ('starting', 'updating', 'destroying')),
  owner_boot_id TEXT NOT NULL,
  lock_device BIGINT NOT NULL,
  lock_inode BIGINT NOT NULL,
  outcome TEXT,
  observation JSONB
);
