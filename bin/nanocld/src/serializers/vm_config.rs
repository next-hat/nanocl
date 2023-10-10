use nanocl_stubs::vm_config::{VmConfigPartial, VmConfig};

use crate::models::VmConfigDbModel;

pub fn serialize_vm_config(
  dbmodel: VmConfigDbModel,
  config: &VmConfigPartial,
) -> VmConfig {
  VmConfig {
    key: dbmodel.key,
    created_at: dbmodel.created_at,
    name: config.name.clone(),
    version: dbmodel.version,
    vm_key: dbmodel.vm_key,
    disk: config.disk.clone(),
    host_config: config.host_config.clone().unwrap_or_default(),
    hostname: config.hostname.clone(),
    user: config.user.clone(),
    labels: config.labels.clone(),
    mac_address: config.mac_address.clone(),
    password: config.password.clone(),
    ssh_key: config.ssh_key.clone(),
    metadata: config.metadata.clone(),
  }
}
