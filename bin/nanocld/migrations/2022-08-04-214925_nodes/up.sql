-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "nodes" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "ip_address" INET NOT NULL UNIQUE,
  "endpoint" VARCHAR NOT NULL,
  "version" VARCHAR NOT NULL,
  "metadata" JSONB
);

CREATE TABLE IF NOT EXISTS "node_groups" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS "node_group_links" (
  "node_name" VARCHAR NOT NULL REFERENCES "nodes" ("name"),
  "node_group_name" VARCHAR NOT NULL REFERENCES "node_groups" ("name")
);

CREATE INDEX "nodes_name_idx" ON "nodes" ("name");
CREATE INDEX "nodes_created_at_idx" ON "nodes" ("created_at");
CREATE INDEX "nodes_ip_address_idx" ON "nodes" ("ip_address");
CREATE INDEX "nodes_endpoint_idx" ON "nodes" ("endpoint");
CREATE INDEX "nodes_version_idx" ON "nodes" ("version");
CREATE INDEX "nodes_metadata_idx" ON "nodes" USING GIN ("metadata");
