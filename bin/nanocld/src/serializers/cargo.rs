use nanocl_stubs::{cargo_config::CargoConfig, cargo::Cargo};

use crate::models::CargoDbModel;

pub fn serialize_cargo(dbmodel: CargoDbModel, config: CargoConfig) -> Cargo {
  Cargo {
    key: dbmodel.key,
    name: dbmodel.name,
    config_key: config.key,
    namespace_name: dbmodel.namespace_name,
    config,
  }
}
