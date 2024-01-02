-- Your SQL goes here
ALTER TABLE IF EXISTS "metrics" RENAME COLUMN "expire_at" TO "expires_at";
