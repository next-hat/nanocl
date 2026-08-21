CREATE TABLE IF NOT EXISTS "networks" (
  "key" VARCHAR NOT NULL PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "name" VARCHAR NOT NULL,
  "node_name" VARCHAR NOT NULL REFERENCES "nodes"("name") ON DELETE CASCADE,
  "data" JSONB NOT NULL,
  "metadata" JSONB,
  UNIQUE ("node_name", "name")
);

CREATE INDEX "networks_created_at_idx" ON "networks" ("created_at");
CREATE INDEX "networks_updated_at_idx" ON "networks" ("updated_at");
CREATE INDEX "networks_name_idx" ON "networks" ("name");
CREATE INDEX "networks_node_name_idx" ON "networks" ("node_name");
CREATE INDEX "networks_data_idx" ON "networks" USING GIN ("data");
CREATE INDEX "networks_metadata_idx" ON "networks" USING GIN ("metadata");
