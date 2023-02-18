-- Your SQL goes here
CREATE TABLE "nodes" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "mode" TEXT NOT NULL,
  "labels" JSON NOT NULL,
  "ip_address" VARCHAR NOT NULL
);
