use ntex::web;

use nanocl_error::http::HttpResult;

use nanocl_stubs::{
  generic::{GenericNspQuery, GenericListNspQuery},
  cargo::{CargoDeleteQuery, CargoKillOptions, CargoStatsQuery, CargoScale},
  cargo_spec::{CargoSpecPartial, CargoSpecUpdate},
};

use crate::{
  utils,
  repositories::generic::*,
  models::{DaemonState, CargoSpecDb, FromSpec},
};

/// List cargoes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"where\": { \"name\": { \"eq\": \"test\" } } }"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargoes are"),
  ),
  responses(
    (status = 200, description = "List of cargoes", body = [CargoSummary]),
  ),
))]
#[web::get("/cargoes")]
pub(crate) async fn list_cargo(
  state: web::types::State<DaemonState>,
  qs: web::types::Query<GenericListNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let cargoes = utils::cargo::list(&qs, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargoes))
}

/// Get detailed information about a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{name}/inspect",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "Cargo details", body = CargoInspect),
  ),
))]
#[web::get("/cargoes/{name}/inspect")]
pub(crate) async fn inspect_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
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
  request_body = CargoSpecPartial,
  params(
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 201, description = "Cargo created", body = Cargo),
  ),
))]
#[web::post("/cargoes")]
pub(crate) async fn create_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<String>,
  payload: web::types::Json<CargoSpecPartial>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let cargo = utils::cargo::create(&namespace, &payload, &path, &state).await?;
  Ok(web::HttpResponse::Created().json(&cargo))
}

/// Delete a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Cargoes",
  path = "/cargoes/{name}",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
    ("force" = bool, Query, description = "If true forces the delete operation"),
  ),
  responses(
    (status = 202, description = "Cargo deleted"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::delete("/cargoes/{name}")]
pub(crate) async fn delete_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<CargoDeleteQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("service::delete_cargo: {key}");
  utils::cargo::delete_by_key(&key, qs.force, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Restart a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  path = "/cargoes/{name}/restart",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 202, description = "Cargo restarted"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/cargoes/{name}/restart")]
pub(crate) async fn restart_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  utils::cargo::restart(&key, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Create a new cargo spec and add history entry
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Cargoes",
  request_body = CargoSpecPartial,
  path = "/cargoes/{name}",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "Cargo updated", body = Cargo),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::put("/cargoes/{name}")]
pub(crate) async fn put_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoSpecPartial>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let cargo = utils::cargo::put(&key, &payload, &path.0, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}

/// Patch a cargo spec meaning merging current spec with the new one and add history entry
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  request_body = CargoSpecUpdate,
  path = "/cargoes/{name}",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "Cargo updated", body = Cargo),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::patch("/cargoes/{name}")]
pub(crate) async fn patch_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoSpecUpdate>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let cargo = utils::cargo::patch(&key, &payload, &path.0, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}

/// Send a signal to a cargo this will kill the cargo if the signal is SIGKILL
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  request_body = CargoKillOptions,
  path = "/cargoes/{name}/kill",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "Cargo killed"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/cargoes/{name}/kill")]
pub(crate) async fn kill_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoKillOptions>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  utils::cargo::kill_by_key(&key, &payload, &state).await?;
  Ok(web::HttpResponse::Ok().into())
}

/// List cargo histories
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{name}/histories",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "List of cargo histories", body = Vec<CargoSpec>),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::get("/cargoes/{name}/histories")]
pub(crate) async fn list_cargo_history(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let histories = CargoSpecDb::find_by_cargo(&key, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&histories))
}

/// Revert a cargo to a specific history
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  path = "/cargoes/{name}/histories/{id}/revert",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("id" = String, Path, description = "Id of the cargo history"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "Cargo revert", body = Cargo),
    (status = 404, description = "Cargo does not exist", body = ApiError),
  ),
))]
#[web::patch("/cargoes/{name}/histories/{id}/revert")]
pub(crate) async fn revert_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, uuid::Uuid)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let cargo_key = utils::key::gen_key(&namespace, &path.1);
  let spec = CargoSpecDb::read_by_pk(&path.2, &state.pool)
    .await??
    .try_to_spec()?;
  let cargo =
    utils::cargo::put(&cargo_key, &spec.clone().into(), &path.0, &state)
      .await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}

/// Get stats of a cargo instance
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{name}/stats",
  params(
    ("name" = String, Path, description = "Name of the cargo instance usually `name` or `name-number`"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
    ("stream" = Option<bool>, Query, description = "Return a stream of stats"),
    ("one_shot" = Option<bool>, Query, description = "Return stats only once"),
  ),
  responses(
    (status = 200, description = "Cargo stats", content_type = "application/vdn.nanocl.raw-stream", body = Stats),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::get("/cargoes/{name}/stats")]
pub(crate) async fn stats_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<CargoStatsQuery>,
) -> HttpResult<web::HttpResponse> {
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
  path = "/cargoes/{name}/scale",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "Cargo scaled", body = Cargo),
    (status = 404, description = "Cargo does not exist", body = ApiError),
  ),
))]
#[web::patch("/cargoes/{name}/scale")]
pub(crate) async fn scale_cargo(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<GenericNspQuery>,
  payload: web::types::Json<CargoScale>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  utils::cargo::scale(&key, &payload, &state).await?;
  Ok(web::HttpResponse::Ok().into())
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_cargo);
  config.service(delete_cargo);
  config.service(restart_cargo);
  config.service(kill_cargo);
  config.service(patch_cargo);
  config.service(put_cargo);
  config.service(list_cargo);
  config.service(inspect_cargo);
  config.service(list_cargo_history);
  config.service(revert_cargo);
  config.service(scale_cargo);
  config.service(stats_cargo);
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPartial};
  use nanocl_stubs::cargo::{
    Cargo, CargoSummary, CargoInspect, CargoDeleteQuery, CargoScale,
    CargoKillOptions,
  };

  use crate::utils::tests::*;
  use crate::services::cargo_image::tests::ensure_test_image;

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
          Some(&CargoSpecPartial {
            name: test_cargo.to_owned(),
            container: bollard_next::container::Config {
              image: Some(
                "ghcr.io/nxthat/nanocl-get-started:latest".to_owned(),
              ),
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
        "basic cargo create"
      );
      let cargo = TestClient::res_json::<Cargo>(res).await;
      assert_eq!(cargo.spec.name, test_cargo, "Invalid cargo name");
      assert_eq!(cargo.namespace_name, "global", "Invalid cargo namespace");
      assert_eq!(
        cargo.spec.container.image,
        Some("ghcr.io/nxthat/nanocl-get-started:latest".to_owned())
      );
    }
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
      response.spec.name, main_test_cargo,
      "Expected to find cargo with name {main_test_cargo} got {}",
      response.spec.name
    );
    let mut res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo list");
    let cargoes = res.json::<Vec<CargoSummary>>().await.unwrap();
    assert!(!cargoes.is_empty(), "Expected to find cargoes");
    let res = client
      .send_post(
        &format!("/processes/cargo/{main_test_cargo}/start"),
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
        Some(&CargoKillOptions {
          signal: "SIGINT".to_owned(),
        }),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo kill");
    let res = client
      .send_get(
        &format!("{ENDPOINT}/{main_test_cargo}/stats"),
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
      http::StatusCode::ACCEPTED,
      "basic cargo restart"
    );
    let mut res = client
      .send_put(
        &format!("{ENDPOINT}/{main_test_cargo}"),
        Some(&CargoSpecPartial {
          name: main_test_cargo.to_owned(),
          container: bollard_next::container::Config {
            image: Some("ghcr.io/nxthat/nanocl-get-started:latest".to_owned()),
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
    assert_eq!(patch_response.spec.name, main_test_cargo);
    assert_eq!(patch_response.namespace_name, "global");
    assert_eq!(
      patch_response.spec.container.image,
      Some("ghcr.io/nxthat/nanocl-get-started:latest".to_owned())
    );
    assert_eq!(
      patch_response.spec.container.env,
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
    let histories = res.json::<Vec<CargoSpec>>().await.unwrap();
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
        &format!("/processes/cargo/{main_test_cargo}/stop"),
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
        Some(&CargoSpecPartial {
          name: CARGO_NAME.to_owned(),
          container: bollard_next::container::Config {
            image: Some("ghcr.io/nxthat/nanocl-get-started:latest".to_owned()),
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
        &format!("/processes/cargo/{CARGO_NAME}/start"),
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
        &format!("/processes/cargo/{CARGO_NAME}/stop"),
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
}
