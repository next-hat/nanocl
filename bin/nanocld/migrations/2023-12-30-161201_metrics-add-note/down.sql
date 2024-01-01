-- This file should undo anything in `up.sql`
ALTER TABLE IF EXISTS metrics DROP COLUMN note;
