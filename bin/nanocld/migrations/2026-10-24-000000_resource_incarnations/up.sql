CREATE TABLE resource_incarnations (
  key UUID PRIMARY KEY,
  resource_kind TEXT NOT NULL,
  resource_key TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  deleted_at TIMESTAMPTZ,
  current_operation_key UUID
);
