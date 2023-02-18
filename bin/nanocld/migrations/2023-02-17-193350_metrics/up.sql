-- Your SQL goes here
CREATE TABLE IF NOT EXISTS metrics (
  key uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
  kind TEXT NOT NULL,
  value JSON NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expire_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '4 month'
) WITH (ttl_expiration_expression = 'ttl');
