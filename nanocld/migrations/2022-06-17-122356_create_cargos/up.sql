-- Your SQL goes here
create table "cargoes" (
  "key" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "name" VARCHAR NOT NULL,
  "config_key" UUID NOT NULL references cargo_configs("key"),
  "namespace_name" VARCHAR NOT NULL references namespaces("name")
);
