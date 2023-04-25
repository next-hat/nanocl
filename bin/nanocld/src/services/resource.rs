/*
* Endpoints to manipulate resources
*/

use ntex::rt;
use ntex::web;

use nanocl_stubs::system::Event;
use nanocl_stubs::resource::ResourcePatch;
use nanocl_stubs::resource::{ResourcePartial, ResourceQuery};

use crate::{utils, repositories};
use nanocl_utils::http_error::HttpError;
use crate::models::{DaemonState, ResourceResetPath};

/// List resources
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources",
  params(
    ("Kind" = Option<String>, Query, description = "Filter by resource kind"),
    ("Contains" = Option<String>, Query, description = "Filter by resource content"),
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
  let items = repositories::resource::find(&state.pool, Some(query)).await?;
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
  utils::resource::delete(resource.clone(), &state.pool).await?;
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
  patch,
  request_body = ResourcePatch,
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
pub(crate) async fn patch_resource(
  web::types::Json(payload): web::types::Json<ResourcePatch>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let resource =
    repositories::resource::inspect_by_key(&path.1, &state.pool).await?;

  let new_resource = ResourcePartial {
    name: path.1.clone(),
    version: payload.version,
    kind: resource.kind,
    config: payload.config,
  };
  let resource = utils::resource::patch(new_resource, &state.pool).await?;
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
    repositories::resource_config::list_by_resource(&path.1, &state.pool)
      .await?;
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Reset a resource to a specific history
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Resources",
  path = "/resources/{Name}/histories/{Id}/reset",
  params(
    ("Name" = String, Path, description = "The resource name to reset"),
    ("Id" = String, Path, description = "The resource history id to reset to")
  ),
  responses(
    (status = 200, description = "The resource has been reset", body = Resource),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::patch("/resources/{name}/histories/{id}/reset")]
pub(crate) async fn reset_resource(
  path: web::types::Path<ResourceResetPath>,
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
    config: history.data,
  };
  let resource = utils::resource::patch(new_resource, &state.pool).await?;
  let resource_ptr = resource.clone();
  rt::spawn(async move {
    let _ = state
      .event_emitter
      .emit(Event::ResourcePatched(Box::new(resource_ptr)))
      .await;
  });
  Ok(web::HttpResponse::Ok().json(&resource))
}

/// Endpoint to allow CORS preflight
#[web::options("/resources{all}*")]
pub(crate) async fn options_resource() -> Result<web::HttpResponse, HttpError> {
  Ok(
    web::HttpResponse::Ok()
      .header("Access-Control-Allow-Origin", "*")
      .header("Access-Control-Allow-Headers", "*")
      .header("Access-Control-Allow-Methods", "*")
      .header("Access-Control-Max-Age", "600")
      .finish(),
  )
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_resource);
  config.service(options_resource);
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
