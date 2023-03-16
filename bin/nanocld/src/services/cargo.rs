/*
* Endpoints to manipulate cargoes
*/

use nanocl_stubs::cargo::CargoKillOptions;
use ntex::rt;
use ntex::web;
use ntex::http::StatusCode;

use nanocl_stubs::system::Event;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::cargo::CargoExecConfig;
use nanocl_stubs::cargo_config::{CargoConfigPartial, CargoConfigUpdate};

use crate::{utils, repositories};
use crate::event::EventEmitterPtr;
use crate::error::HttpResponseError;
use crate::models::{Pool, CargoResetPath};

#[web::post("/cargoes")]
pub async fn create_cargo(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
  event_emitter: web::types::State<EventEmitterPtr>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<CargoConfigPartial>,
  version: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  log::debug!("Creating cargo: {:?}", &payload);
  let cargo = utils::cargo::create(
    &namespace,
    &payload,
    version.into_inner(),
    &docker_api,
    &pool,
  )
  .await?;
  log::debug!("Cargo created: {:?}", &cargo);
  let key = cargo.key.to_owned();
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &docker_api, &pool)
      .await
      .unwrap();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoCreated(Box::new(cargo)));
  });
  Ok(web::HttpResponse::Created().json(&cargo))
}

#[web::delete("/cargoes/{name}")]
pub async fn delete_cargo(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
  event_emitter: web::types::State<EventEmitterPtr>,
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("Deleting cargo: {}", &key);
  let cargo = utils::cargo::inspect(&key, &docker_api, &pool).await?;
  utils::cargo::delete(&key, &docker_api, &pool, None).await?;
  rt::spawn(async move {
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoDeleted(Box::new(cargo)));
  });
  Ok(web::HttpResponse::NoContent().finish())
}

#[web::post("/cargoes/{name}/start")]
pub async fn start_cargo(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
  event_emitter: web::types::State<EventEmitterPtr>,
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("Starting cargo: {}", &key);
  utils::cargo::start(&key, &docker_api, &pool).await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &docker_api, &pool)
      .await
      .unwrap();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoStarted(Box::new(cargo)));
  });
  Ok(web::HttpResponse::Accepted().finish())
}

#[web::post("/cargoes/{name}/stop")]
pub async fn stop_cargo(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
  event_emitter: web::types::State<EventEmitterPtr>,
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("Stopping cargo: {}", &key);
  utils::cargo::inspect(&key, &docker_api, &pool).await?;
  utils::cargo::stop(&key, &docker_api).await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &docker_api, &pool)
      .await
      .unwrap();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoStopped(Box::new(cargo)));
  });
  Ok(web::HttpResponse::Accepted().finish())
}

#[web::put("/cargoes/{name}")]
pub async fn put_cargo(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
  event_emitter: web::types::State<EventEmitterPtr>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoConfigUpdate>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("Patching cargo: {}", &key);
  let cargo =
    utils::cargo::put(&key, &payload, path.0.clone(), &docker_api, &pool)
      .await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &docker_api, &pool)
      .await
      .unwrap();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoPatched(Box::new(cargo)));
  });
  Ok(web::HttpResponse::Ok().json(&cargo))
}

#[web::patch("/cargoes/{name}")]
pub async fn patch_cargo(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
  event_emitter: web::types::State<EventEmitterPtr>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoConfigUpdate>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("Patching cargo: {}", &key);
  let cargo =
    utils::cargo::patch(&key, &payload, path.0.clone(), &docker_api, &pool)
      .await?;
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &docker_api, &pool)
      .await
      .unwrap();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoPatched(Box::new(cargo)));
  });
  Ok(web::HttpResponse::Ok().json(&cargo))
}

#[web::get("/cargoes")]
pub async fn list_cargo(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  log::debug!("Listing cargoes in namespace: {}", &namespace);
  let cargoes = utils::cargo::list(&namespace, &docker_api, &pool).await?;
  log::debug!("Found {} cargoes: {:#?}", &cargoes.len(), &cargoes);
  Ok(web::HttpResponse::Ok().json(&cargoes))
}

#[web::get("/cargoes/{name}/inspect")]
async fn inspect_cargo(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("Inspecting cargo : {}", &key);
  let cargo = utils::cargo::inspect(&key, &docker_api, &pool).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}

#[web::post("/cargoes/{name}/exec")]
async fn exec_command(
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<CargoExecConfig<String>>,
  docker_api: web::types::State<bollard_next::Docker>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("Executing command on cargo : {}", &key);
  utils::cargo::exec_command(&key, &payload, &docker_api).await
}

#[web::post("/cargoes/{name}/kill")]
async fn kill_cargo(
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  web::types::Json(payload): web::types::Json<CargoKillOptions>,
  docker_api: web::types::State<bollard_next::Docker>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("Executing command on cargo : {}", &key);
  utils::cargo::kill(&key, &payload, &docker_api).await?;
  Ok(web::HttpResponse::Ok().into())
}

#[web::get("/cargoes/{name}/histories")]
async fn list_cargo_history(
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let histories = repositories::cargo_config::list_by_cargo(key, &pool).await?;
  Ok(web::HttpResponse::Ok().json(&histories))
}

#[web::patch("/cargoes/{name}/histories/{id}/reset")]
async fn reset_cargo(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
  path: web::types::Path<CargoResetPath>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
  event_emitter: web::types::State<EventEmitterPtr>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let cargo_key = utils::key::gen_key(&namespace, &path.name);
  log::debug!("Resetting cargo : {}", &cargo_key);
  let config_id =
    uuid::Uuid::parse_str(&path.id).map_err(|err| HttpResponseError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("Invalid config id : {err}"),
    })?;
  let config =
    repositories::cargo_config::find_by_key(config_id.to_owned(), &pool)
      .await?;
  let cargo = utils::cargo::put(
    &cargo_key,
    &config.to_owned().into(),
    path.version.clone(),
    &docker_api,
    &pool,
  )
  .await?;
  let key = cargo_key.to_owned();
  rt::spawn(async move {
    let cargo = utils::cargo::inspect(&key, &docker_api, &pool)
      .await
      .unwrap();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoPatched(Box::new(cargo)));
  });
  log::debug!("Resetting cargo : {} done", &cargo_key);
  Ok(web::HttpResponse::Ok().json(&cargo))
}

#[web::get("/cargoes/{name}/logs")]
async fn logs_cargo(
  docker_api: web::types::State<bollard_next::Docker>,
  path: web::types::Path<(String, String)>,
  web::types::Query(qs): web::types::Query<GenericNspQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  log::debug!("Getting cargo logs : {}", &key);
  let steam = utils::cargo::get_logs(&key, &docker_api)?;
  Ok(web::HttpResponse::Ok().streaming(steam))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_cargo);
  config.service(delete_cargo);
  config.service(start_cargo);
  config.service(stop_cargo);
  config.service(patch_cargo);
  config.service(put_cargo);
  config.service(list_cargo);
  config.service(inspect_cargo);
  config.service(list_cargo_history);
  config.service(reset_cargo);
  config.service(exec_command);
  config.service(logs_cargo);
}

#[cfg(test)]
mod tests {

  use crate::services::ntex_config;

  use nanocl_stubs::generic::GenericNspQuery;
  use nanocl_stubs::cargo::CargoExecConfig;
  use ntex::http::StatusCode;
  use futures::{TryStreamExt, StreamExt};
  use nanocl_stubs::cargo::{Cargo, CargoSummary, CargoInspect, OutputLog};
  use nanocl_stubs::cargo_config::{
    CargoConfigPartial, CargoConfigUpdate, CargoConfig,
  };

  use crate::utils::tests::*;
  use crate::services::cargo_image::tests::ensure_test_image;

  /// Test to create start patch stop and delete a cargo with valid data
  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = generate_server(ntex_config).await;
    ensure_test_image().await?;

    const CARGO_NAME: &str = "daemon-test-cargo1";

    let mut res = srv
      .post("/v0.2/cargoes")
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

    let response = res.json::<Cargo>().await?;
    assert_eq!(response.name, CARGO_NAME);
    assert_eq!(response.namespace_name, "global");
    assert_eq!(
      response.config.container.image,
      Some("nexthat/nanocl-get-started:latest".to_string())
    );

    let mut res = srv
      .get(format!("/v0.2/cargoes/{CARGO_NAME}/inspect"))
      .send()
      .await?;
    assert_eq!(res.status(), 200);

    let response = res.json::<CargoInspect>().await?;
    assert_eq!(response.name, CARGO_NAME);

    let mut res = srv.get("/v0.2/cargoes").send().await?;
    assert_eq!(res.status(), 200);
    let cargoes = res.json::<Vec<CargoSummary>>().await?;
    assert!(!cargoes.is_empty());
    assert_eq!(cargoes[0].namespace_name, "global");

    let res = srv
      .post(format!("/v0.2/cargoes/{}/start", response.name))
      .send()
      .await?;
    assert_eq!(res.status(), 202);

    let mut res = srv
      .put(format!("/v0.2/cargoes/{}", response.name))
      .send_json(&CargoConfigUpdate {
        container: Some(bollard_next::container::Config {
          image: Some("nexthat/nanocl-get-started:latest".to_string()),
          env: Some(vec!["TEST=1".to_string()]),
          ..Default::default()
        }),
        ..Default::default()
      })
      .await?;
    assert_eq!(res.status(), 200);

    let patch_response = res.json::<Cargo>().await?;
    assert_eq!(patch_response.name, CARGO_NAME);
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
      .get(format!("/v0.2/cargoes/{}/histories", response.name))
      .send()
      .await?;
    assert_eq!(res.status(), 200);
    let histories = res.json::<Vec<CargoConfig>>().await?;
    assert!(histories.len() > 1);

    let id = histories[0].key;
    let res = srv
      .patch(format!(
        "/v0.2/cargoes/{}/histories/{id}/reset",
        response.name
      ))
      .send()
      .await?;

    assert_eq!(res.status(), 200);

    let res = srv
      .post(format!("/v0.2/cargoes/{}/stop", response.name))
      .send()
      .await?;
    assert_eq!(res.status(), 202);

    let res = srv
      .delete(format!("/v0.2/cargoes/{}", response.name))
      .send()
      .await?;
    assert_eq!(res.status(), 204);

    Ok(())
  }

  #[ntex::test]
  async fn exec() -> TestRet {
    let srv = generate_server(ntex_config).await;

    const CARGO_NAME: &str = "store";

    let res = srv
      .post(format!("/v0.2/cargoes/{CARGO_NAME}/exec"))
      .query(&GenericNspQuery {
        namespace: Some("system".into()),
      })
      .unwrap()
      .send_json(&CargoExecConfig {
        cmd: Some(vec!["ls", "/", "-lra"]),
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
  async fn logs() -> TestRet {
    let srv = generate_server(ntex_config).await;

    const CARGO_NAME: &str = "store";

    let res = srv
      .get(format!("/v0.2/cargoes/{CARGO_NAME}/logs"))
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
