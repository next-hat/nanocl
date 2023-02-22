/*
* Endpoints to manipulate resources
*/
use ntex::{rt, web};

use nanocl_stubs::system::Event;
use nanocl_stubs::resource::{ResourcePartial, ResourceQuery};

use crate::repositories;
use crate::event::EventEmitterPtr;

use crate::error::HttpResponseError;
use crate::models::Pool;

// Endpoint to create a new Resource
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = ResourcePartial,
  path = "/resources",
  params(
  ),
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
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Creating resource: {:?}", &payload);
  let resource = repositories::resource::create(payload, &pool).await?;
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
  name: web::types::Path<String>,
  event_emitter: web::types::State<EventEmitterPtr>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let key = name.into_inner();
  log::debug!("Deleting resource: {}", &key);
  let resource =
    repositories::resource::inspect_by_key(key.to_owned(), &pool).await?;
  repositories::resource::delete_by_key(key.to_owned(), &pool).await?;
  repositories::resource_config::delete_by_resource_key(key.to_owned(), &pool)
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
  name: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let key = name.into_inner();
  log::debug!("Inspecting resource: {}", &key); // item?
  let resource =
    repositories::resource::inspect_by_key(key.to_owned(), &pool).await?;
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
  name: web::types::Path<String>,
  event_emitter: web::types::State<EventEmitterPtr>,
  web::types::Json(payload): web::types::Json<serde_json::Value>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let key = name.into_inner();
  log::debug!("Patching resource: {} with payload: {:?}", &key, &payload);
  let resource =
    repositories::resource::update_by_key(key, payload, &pool).await?;
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
  name: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let key = name.into_inner();
  log::debug!("Listing resource histories: {}", &key);
  let items =
    repositories::resource_config::list_by_resource(key, &pool).await?;
  log::debug!("Resource histories found : {:#?}", &items);
  Ok(web::HttpResponse::Ok().json(&items))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ResourceResetPath {
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

  let resource = repositories::resource::update_by_key(
    path.name.to_owned(),
    history.data,
    &pool,
  )
  .await?;
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
  use super::*;

  use serde_json::json;
  use ntex::http::StatusCode;

  use crate::utils::tests::*;
  use nanocl_stubs::resource::{ResourceKind, Resource, ResourceConfig};

  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = generate_server(ntex_config).await;

    // Create
    let payload = ResourcePartial {
      name: "test_resource".to_owned(),
      kind: ResourceKind::ProxyRule,
      config: json!({"test":"value"}),
    };
    let mut resp = srv.post("/resources").send_json(&payload).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, ResourceKind::ProxyRule);

    // List
    let mut resp = srv.get("/resources").send_json(&payload).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let _ = resp.json::<Vec<Resource>>().await.unwrap();

    // Inspect
    let mut resp = srv.get("/resources/test_resource").send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, ResourceKind::ProxyRule);
    assert_eq!(resource.config, json!({"test":"value"}));

    // History
    let mut resp = srv
      .get("/resources/test_resource/histories")
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
        "/resources/test_resource/histories/{}/reset",
        history.key
      ))
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Patch
    let patch_payload = json!({"test":"new_value"});
    let mut resp = srv
      .patch("/resources/test_resource")
      .send_json(&patch_payload)
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, ResourceKind::ProxyRule);
    assert_eq!(resource.config, json!({"test":"new_value"}));

    // Delete
    let resp = srv.delete("/resources/test_resource").send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    Ok(())
  }
}
