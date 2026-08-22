use ntex::web;

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoError,
};

use crate::{
  models::{CargoDb, CargoObjPutIn, SpecDb, SystemState},
  objects::generic::*,
  repositories::generic::*,
  utils,
};

fn is_cargo_history_for(stored: &SpecDb, cargo_key: &str) -> bool {
  stored.kind_name == "Cargo" && stored.kind_key == cargo_key
}

/// Revert a cargo to a specific history record
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  path = "/cargoes/{cargo_key}/histories/{history_key}/revert",
  params(
    ("cargo_key" = String, Path, description = "Canonical cargo key in `{namespace}.{name}` format"),
    ("history_key" = String, Path, description = "Key of the cargo history"),
  ),
  responses(
    (status = 200, description = "Cargo revert", body = nanocl_stubs::cargo::Cargo),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::patch("/cargoes/{cargo_key}/histories/{history_key}/revert")]
pub async fn revert_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let cargo_key = utils::key::parse_resource_key(&path.1)?;
  let stored = SpecDb::read_by_pk(&path.2, &state.inner.pool).await?;
  if !is_cargo_history_for(&stored, cargo_key.as_str()) {
    return Err(HttpError::bad_request(
      "Cannot revert a cargo using a history from another cargo",
    ));
  }
  let spec = stored.try_to_cargo_spec()?;
  if spec.name != cargo_key.name() {
    return Err(HttpError::bad_request(
      "Cannot revert a cargo to a history with another resource name",
    ));
  }
  spec.validate().map_err(|error| {
    IoError::invalid_input("CargoSpec".to_owned(), error.to_string())
  })?;
  let obj = &CargoObjPutIn {
    spec,
    version: path.0.clone(),
  };
  let cargo = CargoDb::put_obj_by_pk(cargo_key.as_str(), obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn history(kind_name: &str, kind_key: &str) -> SpecDb {
    SpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      kind_name: kind_name.to_owned(),
      kind_key: kind_key.to_owned(),
      version: "test".to_owned(),
      data: serde_json::Value::Null,
      metadata: None,
    }
  }

  #[test]
  fn cargo_history_target_requires_exact_kind_and_key() {
    assert!(is_cargo_history_for(
      &history("Cargo", "global.api"),
      "global.api"
    ));
    assert!(!is_cargo_history_for(
      &history("Vm", "global.api"),
      "global.api"
    ));
    assert!(!is_cargo_history_for(
      &history("Cargo", "other.api"),
      "global.api"
    ));
  }
}
