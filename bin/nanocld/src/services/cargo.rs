/*
* Endpoints to manipulate cargoes
*/

use nanocl_stubs::cargo::CargoScale;
use ntex::rt;
use ntex::web;
use ntex::http::StatusCode;

use bollard_next::exec::CreateExecOptions;

use nanocl_stubs::system::Event;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::cargo::{
  CargoListQuery, CargoDeleteQuery, CargoKillOptions, CargoLogQuery,
};
use nanocl_stubs::cargo_config::{CargoConfigPartial, CargoConfigUpdate};

use nanocl_utils::http_error::HttpError;

use crate::{utils, repositories};
use crate::models::{DaemonState, CargoRevertPath};

/// List cargoes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes",
  params(
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
    ("Name" = Option<String>, Query, description = "Filter for cargoes with similar name"),
    ("Limit" = Option<i64>, Query, description = "Max amount of cargoes in response"),
    ("Offset" = Option<i64>, Query, description = "Offset of the first cargo in response"),
  ),
  responses(
    (status = 200, description = "List of cargoes", body = [CargoSummary]),
  ),
))]
#[web::get("/cargoes")]
pub(crate) async fn list_cargo(
  web::types::Query(qs): web::types::Query<CargoListQuery>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let query = qs.merge(namespace.as_str());
  let cargoes = utils::cargo::list(query, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargoes))
}

/// List cargo instances
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{Name}/instances",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "List of cargo instances", body = [ContainerSummary]),
  ),
))]
#[web::get("/cargoes/{name}/instances")]
pub(crate) async fn list_cargo_instance(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let instances = utils::cargo::list_instance(&key, &state.docker_api).await?;
  Ok(web::HttpResponse::Ok().json(&instances))
}

/// Inspect a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{Name}/inspect",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Cargo details", body = CargoInspect),
  ),
))]
#[web::get("/cargoes/{name}/inspect")]
async fn inspect_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let cargo = utils::cargo::inspect(&key, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}

/// Create a new cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  path = "/cargoes",
  request_body = CargoConfigPartial,
  params(
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 201, description = "Cargo created", body = CargoInspect),
  ),
))]
#[web::post("/cargoes")]
pub(crate) async fn create_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<CargoConfigPartial>,
  version: web::types::Path<String>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let cargo =
    utils::cargo::create(&namespace, &payload, &version, &state).await?;
  let key = cargo.key.to_owned();
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &state).await.unwrap();
    let _ = state
      .event_emitter
      .emit(Event::CargoCreated(Box::new(cargo)))
      .await;
  });
  Ok(web::HttpResponse::Created().json(&cargo))
}

/// Delete a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Cargoes",
  path = "/cargoes/{Name}",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
    ("Force" = bool, Query, description = "If true forces the delete operation"),
  ),
  responses(
    (status = 202, description = "Cargo deleted"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::delete("/cargoes/{name}")]
pub(crate) async fn delete_cargo(
  web::types::Query(qs): web::types::Query<CargoDeleteQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let cargo = utils::cargo::inspect(&key, &state).await?;
  utils::cargo::delete(&key, qs.force, &state).await?;
  rt::spawn(async move {
    let _ = state
      .event_emitter
      .emit(Event::CargoDeleted(Box::new(cargo)))
      .await;
  });
  Ok(web::HttpResponse::Accepted().finish())
}

/// Start a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  path = "/cargoes/{Name}/start",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 202, description = "Cargo started"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/cargoes/{name}/start")]
pub(crate) async fn start_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  utils::cargo::start(&key, &state).await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &state).await.unwrap();
    let _ = state
      .event_emitter
      .emit(Event::CargoStarted(Box::new(cargo)))
      .await;
  });
  Ok(web::HttpResponse::Accepted().finish())
}

/// Stop a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  path = "/cargoes/{Name}/stop",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 202, description = "Cargo stopped"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/cargoes/{name}/stop")]
pub(crate) async fn stop_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  utils::cargo::inspect(&key, &state).await?;
  utils::cargo::stop(&key, &state.docker_api).await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &state).await.unwrap();
    let _ = state
      .event_emitter
      .emit(Event::CargoStopped(Box::new(cargo)))
      .await;
  });
  Ok(web::HttpResponse::Accepted().finish())
}

/// Restart a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  path = "/cargoes/{Name}/restart",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 202, description = "Cargo restarted"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/cargoes/{name}/restart")]
pub(crate) async fn restart_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  utils::cargo::inspect(&key, &state).await?;
  utils::cargo::restart(&key, &state.docker_api).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Create a new cargo config and add history entry
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Cargoes",
  request_body = CargoConfigPartial,
  path = "/cargoes/{Name}",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Cargo updated", body = Cargo),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::put("/cargoes/{name}")]
pub(crate) async fn put_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  payload: web::types::Json<CargoConfigPartial>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let cargo = utils::cargo::put(&key, &payload, &path.0, &state).await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &state).await.unwrap();
    let _ = state
      .event_emitter
      .emit(Event::CargoPatched(Box::new(cargo)))
      .await;
  });
  Ok(web::HttpResponse::Ok().json(&cargo))
}

/// Patch a cargo config meaning merging current config with the new one and add history entry
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  request_body = CargoConfigUpdate,
  path = "/cargoes/{Name}",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Cargo updated", body = Cargo),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::patch("/cargoes/{name}")]
pub(crate) async fn patch_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  payload: web::types::Json<CargoConfigUpdate>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let cargo = utils::cargo::patch(&key, &payload, &path.0, &state).await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &state).await.unwrap();
    let _ = state
      .event_emitter
      .emit(Event::CargoPatched(Box::new(cargo)))
      .await;
  });
  Ok(web::HttpResponse::Ok().json(&cargo))
}

/// Execute a command in a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  request_body = CreateExecOptions,
  path = "/cargoes/{Name}/exec",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Event Stream of the command output", content_type = "text/event-stream"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/cargoes/{name}/exec")]
async fn exec_command(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<CreateExecOptions>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  utils::cargo::exec_command(&key, &payload, &state).await
}

/// Send a signal to a cargo this will kill the cargo if the signal is SIGKILL
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  request_body = CargoKillOptions,
  path = "/cargoes/{Name}/kill",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Cargo killed"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/cargoes/{name}/kill")]
async fn kill_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<CargoKillOptions>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  utils::cargo::kill(&key, &payload, &state.docker_api).await?;
  Ok(web::HttpResponse::Ok().into())
}

/// List cargo histories
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{Name}/histories",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "List of cargo histories", body = Vec<CargoConfig>),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::get("/cargoes/{name}/histories")]
async fn list_cargo_history(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let histories =
    repositories::cargo_config::list_by_cargo(&key, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&histories))
}

/// Revert a cargo to a specific history
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  path = "/cargoes/{Name}/histories/{Id}/revert",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Id" = String, Path, description = "Id of the cargo history"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Cargo revert", body = Cargo),
    (status = 404, description = "Cargo does not exist", body = ApiError),
  ),
))]
#[web::patch("/cargoes/{name}/histories/{id}/revert")]
async fn revert_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  path: web::types::Path<CargoRevertPath>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let cargo_key = utils::key::gen_key(&namespace, &path.name);
  let config_id = uuid::Uuid::parse_str(&path.id).map_err(|err| HttpError {
    status: StatusCode::BAD_REQUEST,
    msg: format!("Invalid config id : {err}"),
  })?;
  let config =
    repositories::cargo_config::find_by_key(&config_id, &state.pool).await?;
  let cargo = utils::cargo::put(
    &cargo_key,
    &config.clone().into(),
    &path.version,
    &state,
  )
  .await?;
  let key = cargo_key.clone();
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &state).await.unwrap();
    let _ = state
      .event_emitter
      .emit(Event::CargoPatched(Box::new(cargo)))
      .await;
  });
  Ok(web::HttpResponse::Ok().json(&cargo))
}

/// Get logs of a cargo instance
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{Name}/logs",
  params(
    ("Name" = String, Path, description = "Name of the cargo instance usually `name` or `name-number`"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
    ("Since" = Option<i64>, Query, description = "Only logs returned since timestamp"),
    ("Until" = Option<i64>, Query, description = "Only logs returned until timestamp"),
    ("Timestamps" = Option<bool>, Query, description = "Add timestamps to every log line"),
    ("Follow" = Option<bool>, Query, description = "Boolean to return a stream or not"),
    ("Tail" = Option<String>, Query, description = "Only return the n last (integer) or all (\"all\") logs"),
  ),
  responses(
    (status = 200, description = "Cargo logs", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::get("/cargoes/{name}/logs")]
async fn logs_cargo(
  web::types::Query(qs): web::types::Query<CargoLogQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let stream = utils::cargo::get_logs(&key, &qs, &state.docker_api)?;
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(stream),
  )
}

/// Scale or Downscale number of instances
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  request_body = CargoScale,
  path = "/cargoes/{Name}/scale",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Cargo scaled", body = Cargo),
    (status = 404, description = "Cargo does not exist", body = ApiError),
  ),
))]
#[web::patch("/cargoes/{name}/scale")]
async fn scale_cargo(
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<CargoScale>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  utils::cargo::scale(&key, &payload, &state).await?;
  let key = key.clone();
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &state).await.unwrap();
    let _ = state
      .event_emitter
      .emit(Event::CargoPatched(Box::new(cargo)))
      .await;
  });
  Ok(web::HttpResponse::Ok().into())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_cargo);
  config.service(delete_cargo);
  config.service(start_cargo);
  config.service(stop_cargo);
  config.service(restart_cargo);
  config.service(kill_cargo);
  config.service(patch_cargo);
  config.service(put_cargo);
  config.service(list_cargo);
  config.service(inspect_cargo);
  config.service(list_cargo_history);
  config.service(revert_cargo);
  config.service(exec_command);
  config.service(logs_cargo);
  config.service(list_cargo_instance);
  config.service(scale_cargo);
}

#[cfg(test)]
mod tests {

  use std::time::Duration;

  use ntex::http::StatusCode;
  use ntex::time::sleep;

  use crate::services::ntex_config;
  use nanocl_stubs::generic::GenericNspQuery;
  use futures::{TryStreamExt, StreamExt};
  use nanocl_stubs::cargo::{
    Cargo, CargoSummary, CargoInspect, OutputLog, CreateExecOptions,
    CargoDeleteQuery, CargoListQuery, CargoScale,
  };
  use nanocl_stubs::cargo_config::{CargoConfigPartial, CargoConfig};

  use crate::utils::tests::*;
  use crate::services::cargo_image::tests::ensure_test_image;

  /// Test to create start patch stop and delete a cargo with valid data
  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = generate_server(ntex_config).await;
    ensure_test_image().await?;

    let test_cargoes = vec![
      "daemon-test-cargo1",
      "another-test-cargo",
      "daemon-test-cargo3",
    ];
    let main_test_cargo = test_cargoes[0];

    for test_cargo in test_cargoes.iter() {
      let test_cargo = test_cargo.to_string();
      let mut res = srv
        .post("/v0.8/cargoes")
        .send_json(&CargoConfigPartial {
          name: test_cargo.clone(),
          container: bollard_next::container::Config {
            image: Some("nexthat/nanocl-get-started:latest".to_string()),
            ..Default::default()
          },
          ..Default::default()
        })
        .await?;
      assert_eq!(res.status(), 201);

      let response = res.json::<Cargo>().await?;
      assert_eq!(response.name, test_cargo);
      assert_eq!(response.namespace_name, "global");
      assert_eq!(
        response.config.container.image,
        Some("nexthat/nanocl-get-started:latest".to_string())
      );
    }

    let mut res = srv
      .get("/v0.8/cargoes")
      .query(&CargoListQuery {
        name: Some(test_cargoes.get(1).unwrap().to_string()),
        namespace: None,
        limit: None,
        offset: None,
      })?
      .send()
      .await?;
    assert_eq!(res.status(), 200);
    let cargoes = res.json::<Vec<CargoSummary>>().await?;
    assert_eq!(cargoes[0].name, test_cargoes[1].to_string());

    let mut res = srv
      .get("/v0.8/cargoes")
      .query(&CargoListQuery {
        name: None,
        namespace: None,
        limit: Some(1),
        offset: None,
      })?
      .send()
      .await?;
    assert_eq!(res.status(), 200);
    let cargoes = res.json::<Vec<CargoSummary>>().await?;
    assert_eq!(cargoes.len(), 1);

    let mut res = srv
      .get(format!("/v0.8/cargoes/{main_test_cargo}/inspect"))
      .send()
      .await?;
    assert_eq!(res.status(), 200);

    let response = res.json::<CargoInspect>().await?;
    assert_eq!(response.name, main_test_cargo);

    let mut res = srv.get("/v0.8/cargoes").send().await?;
    assert_eq!(res.status(), 200);
    let cargoes = res.json::<Vec<CargoSummary>>().await?;
    assert!(!cargoes.is_empty());
    assert_eq!(cargoes[0].namespace_name, "global");

    let res = srv
      .post(format!("/v0.8/cargoes/{}/start", response.name))
      .send()
      .await?;
    assert_eq!(res.status(), 202);

    let mut res = srv
      .put(format!("/v0.8/cargoes/{}", response.name))
      .send_json(&CargoConfigPartial {
        name: main_test_cargo.to_string(),
        container: bollard_next::container::Config {
          image: Some("nexthat/nanocl-get-started:latest".to_string()),
          env: Some(vec!["TEST=1".to_string()]),
          ..Default::default()
        },
        ..Default::default()
      })
      .await?;
    assert_eq!(res.status(), 200);

    let patch_response = res.json::<Cargo>().await?;
    assert_eq!(patch_response.name, main_test_cargo);
    assert_eq!(patch_response.namespace_name, "global");
    assert_eq!(
      patch_response.config.container.image,
      Some("nexthat/nanocl-get-started:latest".to_string())
    );
    assert_eq!(
      patch_response.config.container.env,
      Some(vec!["TEST=1".to_string()])
    );

    let mut res = srv
      .get(format!("/v0.8/cargoes/{}/histories", response.name))
      .send()
      .await?;
    assert_eq!(res.status(), 200);
    let histories = res.json::<Vec<CargoConfig>>().await?;
    assert!(histories.len() > 1);

    let id = histories[0].key;
    let res = srv
      .patch(format!(
        "/v0.8/cargoes/{}/histories/{id}/revert",
        response.name
      ))
      .send()
      .await?;

    assert_eq!(res.status(), 200);

    let res = srv
      .post(format!("/v0.8/cargoes/{}/stop", response.name))
      .send()
      .await?;
    assert_eq!(res.status(), 202);

    let res = srv
      .delete(format!("/v0.8/cargoes/{}", response.name))
      .send()
      .await?;
    assert_eq!(res.status(), 202);

    let res = srv
      .delete(format!("/v0.8/cargoes/{}", test_cargoes[1]))
      .query(&CargoDeleteQuery {
        namespace: None,
        force: Some(true),
      })?
      .send()
      .await?;
    assert_eq!(res.status(), 202);
    let res = srv
      .delete(format!("/v0.8/cargoes/{}", test_cargoes[2]))
      .query(&CargoDeleteQuery {
        namespace: None,
        force: Some(true),
      })?
      .send()
      .await?;
    assert_eq!(res.status(), 202);
    Ok(())
  }

  #[ntex::test]
  async fn exec() -> TestRet {
    let srv = generate_server(ntex_config).await;

    const CARGO_NAME: &str = "nstore";

    let res = srv
      .post(format!("/v0.8/cargoes/{CARGO_NAME}/exec"))
      .query(&GenericNspQuery {
        namespace: Some("system".into()),
      })
      .unwrap()
      .send_json(&CreateExecOptions {
        cmd: Some(vec!["ls".into(), "/".into(), "-lra".into()]),
        ..Default::default()
      })
      .await?;

    assert_eq!(res.status(), StatusCode::OK);
    let mut stream = res.into_stream();
    let mut payload = Vec::new();
    while let Some(data) = stream.next().await {
      let Ok(data) = data else {
        break;
      };
      payload.extend_from_slice(&data);
      if data.last() == Some(&b'\n') {
        let _ = serde_json::from_slice::<OutputLog>(&payload)?;
        payload.clear();
      }
    }
    Ok(())
  }

  #[ntex::test]
  async fn scale() -> TestRet {
    let srv = generate_server(ntex_config).await;

    const CARGO_NAME: &str = "api-test-scale";
    let res = srv
      .post("/v0.8/cargoes")
      .send_json(&CargoConfigPartial {
        name: CARGO_NAME.to_string(),
        container: bollard_next::container::Config {
          image: Some("nexthat/nanocl-get-started:latest".to_string()),
          ..Default::default()
        },
        ..Default::default()
      })
      .await?;
    assert_eq!(res.status(), 201);

    let res = srv
      .post(format!("/v0.8/cargoes/{CARGO_NAME}/start"))
      .send()
      .await?;
    assert_eq!(res.status(), 202);

    let res = srv
      .patch(format!("/v0.8/cargoes/{CARGO_NAME}/scale"))
      .send_json(&CargoScale { replicas: 2 })
      .await?;

    assert_eq!(res.status(), 200);

    let res = srv
      .patch(format!("/v0.8/cargoes/{CARGO_NAME}/scale"))
      .send_json(&CargoScale { replicas: -1 })
      .await?;
    assert_eq!(res.status(), 200);

    let res = srv
      .post(format!("/v0.8/cargoes/{CARGO_NAME}/stop"))
      .send()
      .await?;
    assert_eq!(res.status(), 202);

    let res = srv
      .delete(format!("/v0.8/cargoes/{CARGO_NAME}"))
      .send()
      .await?;

    assert_eq!(res.status(), 202);

    Ok(())
  }

  #[ntex::test]
  async fn logs() -> TestRet {
    let srv = generate_server(ntex_config).await;

    const CARGO_NAME: &str = "nstore";

    let res = srv
      .get(format!("/v0.8/cargoes/{CARGO_NAME}/logs"))
      .query(&GenericNspQuery {
        namespace: Some("system".into()),
      })
      .unwrap()
      .send()
      .await?;

    assert_eq!(res.status(), StatusCode::OK);
    let mut stream = res.into_stream();
    let mut payload = Vec::new();
    let data = stream.next().await.unwrap().unwrap();
    payload.extend_from_slice(&data);
    if data.last() == Some(&b'\n') {
      let _ = serde_json::from_slice::<OutputLog>(&payload)?;
      payload.clear();
    }
    Ok(())
  }
}
