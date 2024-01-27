-- This file should undo anything in `up.sql`
ALTER TABLE IF EXISTS "metrics" RENAME COLUMN "expires_at" TO "expire_at";
