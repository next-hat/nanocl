ALTER TABLE specs ADD COLUMN incarnation_key UUID REFERENCES resource_incarnations (key) ON DELETE RESTRICT;
