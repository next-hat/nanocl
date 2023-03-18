/*
* Endpoints to manipulate resources
*/

use ntex::rt;
use ntex::web;

use nanocl_stubs::system::Event;
use nanocl_stubs::resource::ResourcePatch;
use nanocl_stubs::resource::{ResourcePartial, ResourceQuery};

use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::{DaemonState, ResourceResetPath};

#[web::post("/resources")]
pub async fn create_resource(
  web::types::Json(payload): web::types::Json<ResourcePartial>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Creating resource: {:?}", &payload);
  let resource = utils::resource::create(payload, &state.pool).await?;
  log::debug!("Resource created: {:?}", &resource);
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    state
      .event_emitter
      .lock()
      .unwrap()
      .send(Event::ResourceCreated(Box::new(resource_ptr)));
  });
  Ok(web::HttpResponse::Created().json(&resource))
}

#[web::delete("/resources/{name}")]
pub async fn delete_resource(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Deleting resource: {}", &path.1);
  let resource =
    repositories::resource::inspect_by_key(path.1.clone(), &state.pool).await?;
  utils::resource::delete(resource.clone(), &state.pool).await?;
  rt::spawn(async move {
    state
      .event_emitter
      .lock()
      .unwrap()
      .send(Event::ResourceDeleted(Box::new(resource)));
  });
  Ok(web::HttpResponse::Accepted().finish())
}

#[web::get("/resources")]
pub async fn list_resource(
  web::types::Query(query): web::types::Query<ResourceQuery>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Listing resources with query: {query:#?}");
  let items = repositories::resource::find(&state.pool, Some(query)).await?;
  log::debug!("Found {} resources", &items.len());
  Ok(web::HttpResponse::Ok().json(&items))
}

#[web::get("/resources/{name}")]
pub async fn inspect_resource(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Inspecting resource: {}", &path.1); // item?
  let resource =
    repositories::resource::inspect_by_key(path.1.clone(), &state.pool).await?;
  log::debug!("Resource found: {:?}", &resource);
  Ok(web::HttpResponse::Ok().json(&resource))
}

#[web::patch("/resources/{name}")]
pub async fn patch_resource(
  web::types::Json(payload): web::types::Json<ResourcePatch>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!(
    "Patching resource: {} with payload: {:?}",
    &path.1,
    &payload
  );

  let resource =
    repositories::resource::inspect_by_key(path.1.clone(), &state.pool).await?;

  let new_resource = ResourcePartial {
    name: path.1.clone(),
    version: payload.version,
    kind: resource.kind,
    config: payload.config,
  };
  let resource = utils::resource::patch(new_resource, &state.pool).await?;
  log::debug!("Resource patched: {:?}", &resource);
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    state
      .event_emitter
      .lock()
      .unwrap()
      .send(Event::ResourcePatched(Box::new(resource_ptr)));
  });
  Ok(web::HttpResponse::Ok().json(&resource))
}

#[web::get("/resources/{name}/histories")]
pub async fn list_resource_history(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Listing resource histories: {}", &path.1);
  let items =
    repositories::resource_config::list_by_resource(&path.1, &state.pool)
      .await?;
  log::debug!("Resource histories found : {:#?}", &items);
  Ok(web::HttpResponse::Ok().json(&items))
}

#[web::patch("/resources/{name}/histories/{id}/reset")]
pub async fn reset_resource(
  path: web::types::Path<ResourceResetPath>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let history =
    repositories::resource_config::find_by_key(&path.id, &state.pool).await?;

  let resource =
    repositories::resource::inspect_by_key(path.name.to_owned(), &state.pool)
      .await?;

  let new_resource = ResourcePartial {
    name: resource.name,
    version: history.version,
    kind: resource.kind,
    config: history.data,
  };
  let resource = utils::resource::patch(new_resource, &state.pool).await?;
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    state
      .event_emitter
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
  use nanocl_stubs::resource::{Resource, ResourcePartial, ResourcePatch};

  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = generate_server(ntex_config).await;

    let config = serde_json::json!({
      "type": "object",
      "required": [
        "Watch"
      ],
      "properties": {
        "Watch": {
          "description": "Cargo to watch for changes",
          "type": "array",
          "items": {
            "type": "string"
          }
        }
      }
    });

    let resource = ResourcePartial {
      name: "test_resource".to_owned(),
      version: "v0.0.1".to_owned(),
      kind: "Custom".to_owned(),
      config: config.clone(),
    };

    let mut resp = srv
      .post("/v0.2/resources")
      .send_json(&resource)
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, String::from("Custom"));

    // List
    let mut resp = srv.get("/v0.2/resources").send().await.unwrap();
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
    assert_eq!(resource.kind, String::from("Custom"));
    assert_eq!(&resource.config, &config);

    // History
    let _ = srv
      .get("/v0.2/resources/test_resource/histories")
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let new_resource = ResourcePatch {
      version: "v0.0.2".to_owned(),
      config: config.clone(),
    };
    let mut resp = srv
      .patch("/v0.2/resources/test_resource")
      .send_json(&new_resource)
      .await
      .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, String::from("Custom"));

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
