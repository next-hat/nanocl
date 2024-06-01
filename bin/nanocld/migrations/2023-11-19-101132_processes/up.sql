-- Your SQL goes here-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "processes" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "name" VARCHAR NOT NULL,
  "kind" VARCHAR NOT NULL,
  "data" JSONB NOT NULL,
  "node_name" VARCHAR NOT NULL REFERENCES nodes("name"),
  "kind_key" VARCHAR NOT NULL
);

CREATE INDEX "processes_key_idx" ON "processes" ("key");
CREATE INDEX "processes_created_at_idx" ON "processes" ("created_at");
CREATE INDEX "processes_updated_at_idx" ON "processes" ("updated_at");
CREATE INDEX "processes_name_idx" ON "processes" ("name");
CREATE INDEX "processes_kind_idx" ON "processes" ("kind");
CREATE INDEX "processes_data_idx" ON "processes" USING GIN ("data");
CREATE INDEX "processes_node_name_idx" ON "processes" ("node_name");
CREATE INDEX "processes_kind_key_idx" ON "processes" ("kind_key");
