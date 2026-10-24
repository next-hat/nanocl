INSERT INTO resource_incarnations (key, resource_kind, resource_key, created_at)
  SELECT gen_random_uuid(), 'Vm', key, created_at FROM vms;
UPDATE vms SET incarnation_key = resource_incarnations.key
  FROM resource_incarnations
  WHERE resource_incarnations.resource_kind = 'Vm'
    AND resource_incarnations.resource_key = vms.key;
UPDATE specs SET incarnation_key = vms.incarnation_key
  FROM vms WHERE specs.kind_name = 'Vm' AND specs.kind_key = vms.key;
