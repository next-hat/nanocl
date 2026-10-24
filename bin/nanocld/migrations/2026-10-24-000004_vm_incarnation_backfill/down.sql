UPDATE specs SET incarnation_key = NULL WHERE kind_name = 'Vm';
UPDATE vms SET incarnation_key = NULL;
