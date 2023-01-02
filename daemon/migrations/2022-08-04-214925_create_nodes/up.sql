-- Your SQL goes here
CREATE TYPE "node_modes" AS ENUM ('master', 'worker', 'proxy');
CREATE TYPE "ssh_auth_modes" as ENUM ('passwd', 'rsa');

CREATE TABLE "nodes" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "mode" node_modes NOT NULL,
  "ip_address" VARCHAR NOT NULL,
  "ssh_auth_mode" ssh_auth_modes NOT NULL,
  "ssh_user" VARCHAR NOT NULL,
  "ssh_credential" VARCHAR NOT NULL
);
