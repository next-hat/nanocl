/*
* Endpoints to manipulate resources
*/

use nanocl_stubs::resource::ResourcePatch;
use ntex::rt;
use ntex::web;
use nanocl_stubs::system::Event;
use nanocl_stubs::resource::{ResourcePartial, ResourceQuery};

use crate::repositories;
use crate::event::EventEmitterPtr;

use crate::error::HttpResponseError;
use crate::models::Pool;
use crate::utils;

// Endpoint to create a new Resource
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = ResourcePartial,
  path = "/resources",
  responses(
    (status = 201, description = "New resource", body = Resource),
    (status = 400, description = "Generic database error", body = ApiError),
  ),
))]
#[web::post("/resources")]
pub async fn create_resource(
  pool: web::types::State<Pool>,
  web::types::Json(payload): web::types::Json<ResourcePartial>,
  event_emitter: web::types::State<EventEmitterPtr>,
  _version: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Creating resource: {:?}", &payload);
  let resource = utils::resource::create(payload, &pool).await?;
  log::debug!("Resource created: {:?}", &resource);
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    event_emitter
      .lock()
      .unwrap()
      .send(Event::ResourceCreated(Box::new(resource_ptr)));
  });
  Ok(web::HttpResponse::Created().json(&resource))
}

// Endpoint to delete a Resource
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  path = "/resources/{name}",
  params(
    ("name" = String, Path, description = "Name of the resource to delete"),
  ),
  responses(
    (status = 204, description = "Resource deleted"),
    (status = 400, description = "Generic database error", body = ApiError),
    (status = 404, description = "Resource not found", body = ApiError),
  ),
))]
#[web::delete("/resources/{name}")]
pub async fn delete_resource(
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
  event_emitter: web::types::State<EventEmitterPtr>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Deleting resource: {}", &path.1);
  let resource =
    repositories::resource::inspect_by_key(path.1.clone(), &pool).await?;
  repositories::resource::delete_by_key(path.1.clone(), &pool).await?;
  repositories::resource_config::delete_by_resource_key(path.1.clone(), &pool)
    .await?;
  rt::spawn(async move {
    event_emitter
      .lock()
      .unwrap()
      .send(Event::ResourceDeleted(Box::new(resource)));
  });
  Ok(web::HttpResponse::Accepted().finish())
}

/// Endpoint to list resources
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  path = "/resources",
  params(
  ),
  responses(
    (status = 200, description = "Resource list", body = [Resource]),
    (status = 400, description = "Generic database error", body = ApiError),
  ),
))]
#[web::get("/resources")]
pub async fn list_resource(
  pool: web::types::State<Pool>,
  web::types::Query(query): web::types::Query<ResourceQuery>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Listing resources with query: {query:#?}");
  let items = repositories::resource::find(&pool, Some(query)).await?;
  log::debug!("Found {} resources", &items.len());
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Endpoint to inspect a Resource
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  path = "/resources/{name}/inspect",
  params(
    ("name" = String, Path, description = "Name of the resource to inspect"),
  ),
  responses(
    (status = 200, description = "Resource", body = Resource),
    (status = 400, description = "Generic database error", body = ApiError),
    (status = 404, description = "Resource not found", body = ApiError),
  ),
))]
#[web::get("/resources/{name}")]
pub async fn inspect_resource(
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Inspecting resource: {}", &path.1); // item?
  let resource =
    repositories::resource::inspect_by_key(path.1.clone(), &pool).await?;
  log::debug!("Resource found: {:?}", &resource);
  Ok(web::HttpResponse::Ok().json(&resource))
}

/// Endpoint to patch a Resource
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  request_body = ResourcePartial,
  path = "/resources/{name}",
  params(
  ("name" = String, Path, description = "Name of the resource to patch"),
  ),
  responses(
    (status = 200, description = "Resource patched", body = Resource),
    (status = 400, description = "Generic database error", body = ApiError),
    (status = 404, description = "Resource not found", body = ApiError),
  ),
  ))]
#[web::patch("/resources/{name}")]
pub async fn patch_resource(
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
  event_emitter: web::types::State<EventEmitterPtr>,
  web::types::Json(payload): web::types::Json<ResourcePatch>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!(
    "Patching resource: {} with payload: {:?}",
    &path.1,
    &payload
  );

  let resource =
    repositories::resource::inspect_by_key(path.1.clone(), &pool).await?;

  let new_resource = ResourcePartial {
    name: path.1.clone(),
    version: payload.version,
    kind: resource.kind,
    config: payload.config,
  };
  let resource = utils::resource::patch(new_resource, &pool).await?;
  log::debug!("Resource patched: {:?}", &resource);
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    event_emitter
      .lock()
      .unwrap()
      .send(Event::ResourcePatched(Box::new(resource_ptr)));
  });
  Ok(web::HttpResponse::Ok().json(&resource))
}

#[web::get("/resources/{name}/histories")]
pub async fn list_resource_history(
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Listing resource histories: {}", &path.1);
  let items =
    repositories::resource_config::list_by_resource(path.1.clone(), &pool)
      .await?;
  log::debug!("Resource histories found : {:#?}", &items);
  Ok(web::HttpResponse::Ok().json(&items))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ResourceResetPath {
  pub version: String,
  pub name: String,
  pub id: uuid::Uuid,
}

#[web::patch("/resources/{name}/histories/{id}/reset")]
pub async fn reset_resource(
  pool: web::types::State<Pool>,
  path: web::types::Path<ResourceResetPath>,
  event_emitter: web::types::State<EventEmitterPtr>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let history =
    repositories::resource_config::find_by_key(path.id, &pool).await?;

  let resource =
    repositories::resource::inspect_by_key(path.name.to_owned(), &pool).await?;

  let new_resource = ResourcePartial {
    name: resource.name,
    version: history.version,
    kind: resource.kind,
    config: history.data,
  };
  let resource = utils::resource::patch(new_resource, &pool).await?;
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    event_emitter
      .lock()
      .unwrap()
      .send(Event::ResourcePatched(Box::new(resource_ptr)));
  });
  Ok(web::HttpResponse::Ok().json(&resource))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_resource);
  config.service(delete_resource);
  config.service(list_resource);
  config.service(inspect_resource);
  config.service(patch_resource);
  config.service(list_resource_history);
  config.service(reset_resource);
}

#[cfg(test)]
mod tests {

  use crate::services::ntex_config;

  use ntex::http::StatusCode;

  use crate::utils::tests::*;
  use nanocl_stubs::resource::{
    Resource, ResourceConfig, ResourceProxyRule, ProxyRule,
    ProxyStreamProtocol, ProxyTarget, ProxyRuleStream, ResourcePartial,
  };

  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = generate_server(ntex_config).await;

    let config = serde_json::to_value(ResourceProxyRule {
      watch: vec!["random-cargo".into()],
      rule: ProxyRule::Stream(ProxyRuleStream {
        network: "Public".into(),
        protocol: ProxyStreamProtocol::Tcp,
        port: 1234,
        ssl: None,
        target: ProxyTarget {
          key: "random-cargo".into(),
          port: 1234,
        },
      }),
    })
    .unwrap();

    // Create
    let payload = ResourcePartial {
      name: "test_resource".to_owned(),
      kind: String::from("ProxyRule"),
      version: String::from("v0.1"),
      config: config.clone(),
    };
    let mut resp = srv
      .post("/v0.2/resources")
      .send_json(&payload)
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, String::from("ProxyRule"));

    // List
    let mut resp = srv
      .get("/v0.2/resources")
      .send_json(&payload)
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let _ = resp.json::<Vec<Resource>>().await.unwrap();

    // Inspect
    let mut resp = srv
      .get("/v0.2/resources/test_resource")
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, String::from("ProxyRule"));
    assert_eq!(&resource.config, &config);

    // History
    let mut resp = srv
      .get("/v0.2/resources/test_resource/histories")
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let history = resp
      .json::<Vec<ResourceConfig>>()
      .await
      .unwrap()
      .first()
      .unwrap()
      .to_owned();

    // History reset
    let resp = srv
      .patch(format!(
        "/v0.2/resources/test_resource/histories/{}/reset",
        history.key
      ))
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let mut resp = srv
      .patch("/v0.2/resources/test_resource")
      .send_json(&config)
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, String::from("ProxyRule"));

    // Delete
    let resp = srv
      .delete("/v0.2/resources/test_resource")
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    Ok(())
  }
}
