CREATE TABLE IF NOT EXISTS "cargoes" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  "name" VARCHAR NOT NULL,
  "spec_key" UUID NOT NULL REFERENCES specs("key"),
  "namespace_name" VARCHAR NOT NULL REFERENCES namespaces("name")
);
