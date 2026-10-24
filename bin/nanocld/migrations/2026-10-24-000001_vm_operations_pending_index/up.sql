CREATE INDEX IF NOT EXISTS vm_operations_pending_node_idx
  ON vm_operations (node_name, vm_key) WHERE outcome IS NULL;
