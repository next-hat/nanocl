/*
* Endpoints to manipulate cargoes
*/

use ntex::{rt, web, http};

use nanocl_stubs::system::Event;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::cargo::{
  CargoListQuery, CargoDeleteQuery, CargoKillOptions, CargoLogQuery,
  CargoStatsQuery, CargoScale,
};
use nanocl_stubs::cargo_config::{CargoConfigPartial, CargoConfigUpdate};

use nanocl_error::http::HttpError;

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
  let instances = utils::cargo::list_instances(&key, &state.docker_api).await?;
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
  let cargo = utils::cargo::inspect_by_key(&key, &state).await?;
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
    let cargo = utils::cargo::inspect_by_key(&key, &state).await.unwrap();
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
  let cargo = utils::cargo::inspect_by_key(&key, &state).await?;
  utils::cargo::delete_by_key(&key, qs.force, &state).await?;
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
  utils::cargo::start_by_key(&key, &state).await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect_by_key(&key, &state).await.unwrap();
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
  utils::cargo::inspect_by_key(&key, &state).await?;
  utils::cargo::stop_by_key(&key, &state.docker_api).await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect_by_key(&key, &state).await.unwrap();
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
  utils::cargo::inspect_by_key(&key, &state).await?;
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
    let cargo = utils::cargo::inspect_by_key(&key, &state).await.unwrap();
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
    let cargo = utils::cargo::inspect_by_key(&key, &state).await.unwrap();
    let _ = state
      .event_emitter
      .emit(Event::CargoPatched(Box::new(cargo)))
      .await;
  });
  Ok(web::HttpResponse::Ok().json(&cargo))
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
  utils::cargo::kill_by_name(&key, &payload, &state.docker_api).await?;
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
    repositories::cargo_config::list_by_cargo_key(&key, &state.pool).await?;
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
    status: http::StatusCode::BAD_REQUEST,
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
    let cargo = utils::cargo::inspect_by_key(&key, &state).await.unwrap();
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

/// Get stats of a cargo instance
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{Name}/stats",
  params(
    ("Name" = String, Path, description = "Name of the cargo instance usually `name` or `name-number`"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
    ("Stream" = Option<bool>, Query, description = "Only logs returned since timestamp"),
    ("OneShot" = Option<bool>, Query, description = "Only logs returned until timestamp"),
  ),
  responses(
    (status = 200, description = "Cargo stats", content_type = "application/vdn.nanocl.raw-stream", body = Stats),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::get("/cargoes/{name}/stats")]
async fn stats_cargo(
  web::types::Query(qs): web::types::Query<CargoStatsQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let stream = utils::cargo::get_stats(&key, &qs, &state.docker_api)?;
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
    let cargo = utils::cargo::inspect_by_key(&key, &state).await.unwrap();
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
  config.service(logs_cargo);
  config.service(list_cargo_instance);
  config.service(scale_cargo);
  config.service(stats_cargo);
}

#[cfg(test)]
mod tests {
  use bollard_next::container::KillContainerOptions;
  use ntex::http;
  use futures::{TryStreamExt, StreamExt};

  use nanocl_stubs::generic::GenericNspQuery;
  use nanocl_stubs::cargo_config::{CargoConfig, CargoConfigPartial};
  use nanocl_stubs::cargo::{
    Cargo, CargoSummary, CargoInspect, OutputLog, CargoDeleteQuery,
    CargoListQuery, CargoScale,
  };

  use crate::services::cargo_image::tests::ensure_test_image;
  use crate::utils::tests::*;

  const ENDPOINT: &str = "/cargoes";

  /// Test to create start patch stop and delete a cargo with valid data
  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    ensure_test_image().await;
    let test_cargoes = [
      "1daemon-test-cargo",
      "2another-test-cargo",
      "2daemon-test-cargo",
    ];
    let main_test_cargo = test_cargoes[0];
    for test_cargo in test_cargoes.iter() {
      let test_cargo = test_cargo.to_owned();
      let res = client
        .send_post(
          ENDPOINT,
          Some(&CargoConfigPartial {
            name: test_cargo.to_owned(),
            container: bollard_next::container::Config {
              image: Some("nexthat/nanocl-get-started:latest".to_owned()),
              ..Default::default()
            },
            ..Default::default()
          }),
          None::<String>,
        )
        .await;
      let status = res.status();
      test_status_code!(
        status,
        http::StatusCode::CREATED,
        "basic cargo create"
      );
      let cargo = TestClient::res_json::<Cargo>(res).await;
      assert_eq!(cargo.name, test_cargo, "Invalid cargo name");
      assert_eq!(cargo.namespace_name, "global", "Invalid cargo namespace");
      assert_eq!(
        cargo.config.container.image,
        Some("nexthat/nanocl-get-started:latest".to_owned())
      );
    }
    let mut res = client
      .send_get(
        ENDPOINT,
        Some(&CargoListQuery {
          name: Some(test_cargoes[1].to_owned()),
          namespace: None,
          limit: None,
          offset: None,
        }),
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo list filter name"
    );
    let cargoes = res.json::<Vec<CargoSummary>>().await.unwrap();
    assert_eq!(
      cargoes[0].name, test_cargoes[1],
      "Expected to find cargo with name {} got {}",
      test_cargoes[1], cargoes[0].name
    );
    let mut res = client
      .send_get(
        ENDPOINT,
        Some(&CargoListQuery {
          name: None,
          namespace: None,
          limit: Some(1),
          offset: None,
        }),
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo list limit 1"
    );
    let cargoes = res.json::<Vec<CargoSummary>>().await.unwrap();
    let len = cargoes.len();
    assert_eq!(len, 1, "Expected to find 1 cargo got {len}");
    let mut res = client
      .send_get(
        &format!("{ENDPOINT}/{main_test_cargo}/inspect"),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo inspect"
    );
    let response = res.json::<CargoInspect>().await.unwrap();
    assert_eq!(
      response.name, main_test_cargo,
      "Expected to find cargo with name {main_test_cargo} got {}",
      response.name
    );
    let mut res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo list");
    let cargoes = res.json::<Vec<CargoSummary>>().await.unwrap();
    assert!(!cargoes.is_empty(), "Expected to find cargoes");
    let res = client
      .send_post(
        &format!("{ENDPOINT}/{main_test_cargo}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "basic cargo start"
    );
    let res = client
      .send_post(
        &format!("{ENDPOINT}/{main_test_cargo}/kill"),
        Some(&KillContainerOptions { signal: "SIGINT" }),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo kill");

    let res = client
      .send_post(
        &format!("{ENDPOINT}/{main_test_cargo}/stats"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo stats");
    let res = client
      .send_post(
        &format!("{ENDPOINT}/{main_test_cargo}/restart"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo restart"
    );
    let mut res = client
      .send_put(
        &format!("{ENDPOINT}/{main_test_cargo}"),
        Some(&CargoConfigPartial {
          name: main_test_cargo.to_owned(),
          container: bollard_next::container::Config {
            image: Some("nexthat/nanocl-get-started:latest".to_owned()),
            env: Some(vec!["TEST=1".to_owned()]),
            ..Default::default()
          },
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo patch");
    let patch_response = res.json::<Cargo>().await.unwrap();
    assert_eq!(patch_response.name, main_test_cargo);
    assert_eq!(patch_response.namespace_name, "global");
    assert_eq!(
      patch_response.config.container.image,
      Some("nexthat/nanocl-get-started:latest".to_owned())
    );
    assert_eq!(
      patch_response.config.container.env,
      Some(vec!["TEST=1".to_owned()])
    );
    let mut res = client
      .send_get(
        &format!("{ENDPOINT}/{main_test_cargo}/histories"),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo history"
    );
    let histories = res.json::<Vec<CargoConfig>>().await.unwrap();
    assert!(histories.len() > 1, "Expected to find cargo histories");
    let id = histories[0].key;
    let res = client
      .send_patch(
        &format!("{ENDPOINT}/{main_test_cargo}/histories/{id}/revert"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo revert");
    let res = client
      .send_post(
        &format!("{ENDPOINT}/{main_test_cargo}/stop"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "basic cargo stop"
    );
    for test_cargo in test_cargoes.iter() {
      let res = client
        .send_delete(
          &format!("{ENDPOINT}/{test_cargo}"),
          Some(CargoDeleteQuery {
            force: Some(true),
            ..Default::default()
          }),
        )
        .await;
      test_status_code!(
        res.status(),
        http::StatusCode::ACCEPTED,
        "basic cargo delete"
      );
    }
  }

  #[ntex::test]
  async fn scale() {
    const CARGO_NAME: &str = "api-test-scale";
    let client = gen_default_test_client().await;
    let res = client
      .send_post(
        ENDPOINT,
        Some(&CargoConfigPartial {
          name: CARGO_NAME.to_owned(),
          container: bollard_next::container::Config {
            image: Some("nexthat/nanocl-get-started:latest".to_owned()),
            ..Default::default()
          },
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::CREATED,
      "scale cargo create"
    );
    let res = client
      .send_post(
        &format!("{ENDPOINT}/{CARGO_NAME}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "scale cargo start"
    );
    let res = client
      .send_patch(
        &format!("{ENDPOINT}/{CARGO_NAME}/scale"),
        Some(&CargoScale { replicas: 2 }),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "scale cargo scale up"
    );
    let res = client
      .send_patch(
        &format!("{ENDPOINT}/{CARGO_NAME}/scale"),
        Some(&CargoScale { replicas: -1 }),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "scale cargo scale down"
    );
    let res = client
      .send_post(
        &format!("{ENDPOINT}/{CARGO_NAME}/stop"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "scale cargo stop"
    );
    let res = client
      .send_delete(&format!("{ENDPOINT}/{CARGO_NAME}"), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "scale cargo delete"
    );
  }

  #[ntex::test]
  async fn logs() {
    const CARGO_NAME: &str = "nstore";
    let client = gen_default_test_client().await;
    let res = client
      .send_get(
        &format!("{ENDPOINT}/{CARGO_NAME}/logs"),
        Some(&GenericNspQuery {
          namespace: Some("system".into()),
        }),
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "logs cargo logs");
    let mut stream = res.into_stream();
    let mut payload = Vec::new();
    let data = stream.next().await.unwrap().unwrap();
    payload.extend_from_slice(&data);
    if data.last() == Some(&b'\n') {
      let _ = serde_json::from_slice::<OutputLog>(&payload).unwrap();
      payload.clear();
    }
  }
}
