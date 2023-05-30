-- Your SQL goes here
DROP TABLE resource_kind_versions;
CREATE TABLE IF NOT EXISTS "resource_kind_versions" (
  "resource_kind_name" VARCHAR NOT NULL REFERENCES resource_kinds("name"),
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "version" VARCHAR NOT NULL,
  "schema" JSON,
  "url" VARCHAR,
  PRIMARY KEY ("resource_kind_name", "version")
);
