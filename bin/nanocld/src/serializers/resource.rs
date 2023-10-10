use nanocl_stubs::resource::Resource;

use crate::models::{ResourceDbModel, ResourceConfigDbModel};

pub fn serialize_resource(
  dbmodel: ResourceDbModel,
  config: ResourceConfigDbModel,
) -> Resource {
  Resource {
    name: dbmodel.key,
    created_at: dbmodel.created_at,
    updated_at: config.created_at,
    kind: dbmodel.kind,
    version: config.version,
    config_key: config.key,
    data: config.data,
    metadata: config.metadata,
  }
}
