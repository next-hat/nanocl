ALTER TABLE vms ADD COLUMN incarnation_key UUID NOT NULL REFERENCES resource_incarnations (key) ON DELETE RESTRICT;
