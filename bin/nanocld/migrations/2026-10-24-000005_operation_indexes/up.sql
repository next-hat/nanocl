CREATE UNIQUE INDEX resource_incarnations_live_idx ON resource_incarnations (resource_kind, resource_key) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX operations_pending_incarnation_idx ON operations (incarnation_key) WHERE outcome IS NULL;
CREATE INDEX operations_pending_node_idx ON operations (resource_kind, node_name, key) WHERE outcome IS NULL;
CREATE INDEX operations_history_idx ON operations (resource_kind, incarnation_key, accepted_at, key);
CREATE INDEX specs_incarnation_idx ON specs (incarnation_key, created_at);
