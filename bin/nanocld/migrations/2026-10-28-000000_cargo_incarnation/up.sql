-- Fresh-cluster schema: existing installations must be recreated from Statefiles.
ALTER TABLE cargoes ADD COLUMN incarnation_key UUID NOT NULL REFERENCES resource_incarnations (key) ON DELETE RESTRICT;
