-- Your SQL goes here
CREATE TABLE "namespaces" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY
);

INSERT INTO "namespaces" ("name") VALUES ('global');
INSERT INTO "namespaces" ("name") VALUES ('system');
