use ntex::web;
use ntex::http::StatusCode;
use diesel::prelude::*;

use nanocl_models::cargo_config::{CargoConfig, CargoConfigPartial};

use crate::controllers;
use crate::errors::HttpResponseError;
use crate::models::{Pool, CargoConfigDbModel};
use crate::repositories::errors::db_blocking_error;

pub async fn create(
  cargo_key: String,
  item: CargoConfigPartial,
  pool: &Pool,
) -> Result<CargoConfig, HttpResponseError> {
  use crate::schema::cargo_configs::dsl;

  let dbmodel = CargoConfigDbModel {
    key: uuid::Uuid::new_v4(),
    cargo_key,
    config: serde_json::to_value(item.to_owned()).map_err(|e| {
      HttpResponseError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Failed to serialize config: {}", e),
      }
    })?,
  };
  let mut conn = controllers::store::get_pool_conn(pool)?;
  let dbmodel = web::block(move || {
    diesel::insert_into(dsl::cargo_configs)
      .values(&dbmodel)
      .execute(&mut conn)?;
    Ok(dbmodel)
  })
  .await
  .map_err(db_blocking_error)?;

  let config = CargoConfig {
    key: dbmodel.key,
    name: item.name,
    cargo_key: dbmodel.cargo_key,
    dns_entry: item.dns_entry,
    replication: item.replication,
    container: item.container,
  };

  Ok(config)
}

pub async fn find_by_key(
  key: uuid::Uuid,
  pool: &Pool,
) -> Result<CargoConfig, HttpResponseError> {
  use crate::schema::cargo_configs::dsl;

  let mut conn = controllers::store::get_pool_conn(pool)?;
  let dbmodel = web::block(move || {
    dsl::cargo_configs
      .filter(dsl::key.eq(key))
      .first::<CargoConfigDbModel>(&mut conn)
  })
  .await
  .map_err(db_blocking_error)?;

  let config = serde_json::from_value::<CargoConfigPartial>(dbmodel.config)
    .map_err(|e| HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to deserialize config: {}", e),
    })?;

  Ok(CargoConfig {
    key: dbmodel.key,
    name: config.name,
    cargo_key: dbmodel.cargo_key,
    dns_entry: config.dns_entry,
    replication: config.replication,
    container: config.container,
  })
}
