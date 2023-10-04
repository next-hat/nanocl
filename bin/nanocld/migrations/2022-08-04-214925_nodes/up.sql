-- Your SQL goes here
CREATE TABLE IF NOT EXISTS "nodes" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "ip_address" VARCHAR NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS "node_groups" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS "node_group_links" (
  "node_name" VARCHAR NOT NULL REFERENCES "nodes" ("name"),
  "node_group_name" VARCHAR NOT NULL REFERENCES "node_groups" ("name")
);
