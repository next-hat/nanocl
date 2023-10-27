/*
* Endpoints to manipulate resources
*/

use ntex::rt;
use ntex::web;

use nanocl_stubs::system::Event;
use nanocl_stubs::resource::ResourceUpdate;
use nanocl_stubs::resource::{ResourcePartial, ResourceQuery};

use crate::{utils, repositories};
use nanocl_utils::http_error::HttpError;
use crate::models::{DaemonState, ResourceRevertPath};

/// List resources
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources",
  params(
    ("Kind" = Option<String>, Query, description = "Filter by resource kind"),
    ("Exists" = Option<String>, Query, description = "Filter by resource by existing key in data"),
    ("Contains" = Option<String>, Query, description = "Filter by resource data"),
    ("MetaContains" = Option<String>, Query, description = "Filter by resource metadata"),
    ("MetaExists" = Option<String>, Query, description = "Filter by resource existing key in metadata"),
  ),
  responses(
    (status = 200, description = "List of resources", body = [Resource]),
  ),
))]
#[web::get("/resources")]
pub(crate) async fn list_resource(
  web::types::Query(query): web::types::Query<ResourceQuery>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let items = repositories::resource::find(Some(query), &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Get detailed information about a resource
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources/{Name}",
  params(
    ("Name" = String, Path, description = "The resource name to inspect")
  ),
  responses(
    (status = 200, description = "Detailed information about a resource", body = Resource),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::get("/resources/{name}")]
pub(crate) async fn inspect_resource(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let resource =
    repositories::resource::inspect_by_key(&path.1, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&resource))
}

/// Create a resource
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = ResourcePartial,
  tag = "Resources",
  path = "/resources",
  responses(
    (status = 200, description = "The created resource", body = Resource),
  ),
))]
#[web::post("/resources")]
pub(crate) async fn create_resource(
  web::types::Json(payload): web::types::Json<ResourcePartial>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let resource = utils::resource::create(&payload, &state.pool).await?;
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    let _ = state
      .event_emitter
      .emit(Event::ResourceCreated(Box::new(resource_ptr)))
      .await;
  });
  Ok(web::HttpResponse::Created().json(&resource))
}

/// Delete a resource
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Resources",
  path = "/resources/{Name}",
  params(
    ("Name" = String, Path, description = "The resource name to delete")
  ),
  responses(
    (status = 202, description = "The resource and his history has been deleted"),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::delete("/resources/{name}")]
pub(crate) async fn delete_resource(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let resource =
    repositories::resource::inspect_by_key(&path.1, &state.pool).await?;
  utils::resource::delete(&resource, &state.pool).await?;
  rt::spawn(async move {
    let _ = state
      .event_emitter
      .emit(Event::ResourceDeleted(Box::new(resource)))
      .await;
  });
  Ok(web::HttpResponse::Accepted().finish())
}

/// Patch a resource (update its version and/or config) and create a new history
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  request_body = ResourceUpdate,
  tag = "Resources",
  path = "/resources/{Name}",
  params(
    ("Name" = String, Path, description = "The resource name to patch")
  ),
  responses(
    (status = 200, description = "The patched resource", body = Resource),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::patch("/resources/{name}")]
pub(crate) async fn put_resource(
  web::types::Json(payload): web::types::Json<ResourceUpdate>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let resource =
    repositories::resource::inspect_by_key(&path.1, &state.pool).await?;

  let new_resource = ResourcePartial {
    name: path.1.clone(),
    version: payload.version,
    kind: resource.kind,
    data: payload.data,
    metadata: payload.metadata,
  };
  let resource = utils::resource::patch(&new_resource, &state.pool).await?;
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    let _ = state
      .event_emitter
      .emit(Event::ResourcePatched(Box::new(resource_ptr)))
      .await;
  });
  Ok(web::HttpResponse::Ok().json(&resource))
}

/// List resource history
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources/{Name}/histories",
  params(
    ("Name" = String, Path, description = "The resource name to list history")
  ),
  responses(
    (status = 200, description = "The resource history", body = [ResourceConfig]),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::get("/resources/{name}/histories")]
pub(crate) async fn list_resource_history(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let items =
    repositories::resource_config::list_by_resource_key(&path.1, &state.pool)
      .await?;
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Revert a resource to a specific history
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Resources",
  path = "/resources/{Name}/histories/{Id}/revert",
  params(
    ("Name" = String, Path, description = "The resource name to revert"),
    ("Id" = String, Path, description = "The resource history id to revert to")
  ),
  responses(
    (status = 200, description = "The resource has been revert", body = Resource),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::patch("/resources/{name}/histories/{id}/revert")]
pub(crate) async fn revert_resource(
  path: web::types::Path<ResourceRevertPath>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let history =
    repositories::resource_config::find_by_key(&path.id, &state.pool).await?;

  let resource =
    repositories::resource::inspect_by_key(&path.name, &state.pool).await?;

  let new_resource = ResourcePartial {
    name: resource.name,
    version: history.version,
    kind: resource.kind,
    data: history.data,
    metadata: history.metadata,
  };
  let resource = utils::resource::patch(&new_resource, &state.pool).await?;
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    let _ = state
      .event_emitter
      .emit(Event::ResourcePatched(Box::new(resource_ptr)))
      .await;
  });
  Ok(web::HttpResponse::Ok().json(&resource))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_resource);
  config.service(delete_resource);
  config.service(list_resource);
  config.service(inspect_resource);
  config.service(put_resource);
  config.service(list_resource_history);
  config.service(revert_resource);
}

#[cfg(test)]
mod tests {

  use crate::services::ntex_config;

  use ntex::http;

  use crate::utils::tests::*;
  use nanocl_stubs::resource::{
    Resource, ResourcePartial, ResourceUpdate, ResourceQuery,
  };

  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = gen_server(ntex_config).await;
    let config = serde_json::json!({
      "Schema": {
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
      }
    });
    let resource = ResourcePartial {
      name: "test_resource".to_owned(),
      version: "v0.0.1".to_owned(),
      kind: "Kind".to_owned(),
      data: config.clone(),
      metadata: Some(serde_json::json!({
        "Test": "gg",
      })),
    };
    let mut resp = srv
      .post("/v0.10/resources")
      .send_json(&resource)
      .await
      .unwrap();
    assert_eq!(resp.status(), http::StatusCode::CREATED);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, String::from("Kind"));
    // Basic list
    let mut resp = srv.get("/v0.10/resources").send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let _ = resp.json::<Vec<Resource>>().await.unwrap();
    // Using filter exists
    let mut resp = srv
      .get("/v0.10/resources")
      .query(&ResourceQuery {
        exists: Some(String::from("Schema")),
        ..Default::default()
      })
      .unwrap()
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let resources = resp.json::<Vec<Resource>>().await.unwrap();
    println!("Filter resource result:\n{resources:?}");
    assert!(resources.len() == 1, "Unable to filter by exists");
    // Inspect
    let mut resp = srv
      .get("/v0.10/resources/test_resource")
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, String::from("Kind"));
    assert_eq!(&resource.data, &config);
    // History
    let _ = srv
      .get("/v0.10/resources/test_resource/histories")
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let new_resource = ResourceUpdate {
      version: "v0.0.2".to_owned(),
      data: config.clone(),
      metadata: None,
    };
    let mut resp = srv
      .patch("/v0.10/resources/test_resource")
      .send_json(&new_resource)
      .await
      .unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, String::from("Kind"));
    // Delete
    let resp = srv
      .delete("/v0.10/resources/test_resource")
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), http::StatusCode::ACCEPTED);
    Ok(())
  }
}
