ALTER TABLE resource_incarnations ADD CONSTRAINT current_operation_incarnation_fk
  FOREIGN KEY (key, current_operation_key) REFERENCES operations (incarnation_key, key) ON DELETE RESTRICT;
