use nanocl_stubs::cargo_config::{CargoConfigPartial, CargoConfig};
use crate::models::CargoConfigDbModel;

pub fn serialize_cargo_config(
  dbmodel: CargoConfigDbModel,
  config: &CargoConfigPartial,
) -> CargoConfig {
  CargoConfig {
    key: dbmodel.key,
    created_at: dbmodel.created_at,
    name: config.name,
    version: dbmodel.version,
    cargo_key: dbmodel.cargo_key,
    replication: config.replication,
    container: config.container,
    metadata: config.metadata,
    secrets: config.secrets,
  }
}
