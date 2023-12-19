/*
* Endpoints to manipulate resources
*/
use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause, GenericListQuery},
  resource::{ResourceSpec, ResourcePartial, ResourceUpdate},
};

use crate::{
  utils,
  repositories::generic::*,
  models::{DaemonState, ResourceSpecDb, Repository, ResourceDb},
};

/// List resources
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"where\": { \"kind\": { \"eq\": \"ProxyRule\" } } }"),
  ),
  responses(
    (status = 200, description = "List of resources", body = [Resource]),
  ),
))]
#[web::get("/resources")]
pub(crate) async fn list_resource(
  state: web::types::State<DaemonState>,
  query: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::try_from(query.into_inner())
    .map_err(|err| HttpError::bad_request(err.to_string()))?;
  let items = ResourceDb::find(&filter, &state.pool).await??;
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Get detailed information about a resource
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources/{name}",
  params(
    ("name" = String, Path, description = "The resource name to inspect")
  ),
  responses(
    (status = 200, description = "Detailed information about a resource", body = Resource),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::get("/resources/{name}")]
pub(crate) async fn inspect_resource(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let resource = ResourceDb::inspect_by_pk(&path.1, &state.pool).await?;
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
  state: web::types::State<DaemonState>,
  payload: web::types::Json<ResourcePartial>,
) -> HttpResult<web::HttpResponse> {
  let resource = utils::resource::create(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&resource))
}

/// Delete a resource
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Resources",
  path = "/resources/{name}",
  params(
    ("name" = String, Path, description = "The resource name to delete")
  ),
  responses(
    (status = 202, description = "The resource and his history has been deleted"),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::delete("/resources/{name}")]
pub(crate) async fn delete_resource(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  utils::resource::delete_by_key(&path.1, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Patch a resource (update its version and/or spec) and create a new history
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  request_body = ResourceUpdate,
  tag = "Resources",
  path = "/resources/{name}",
  params(
    ("name" = String, Path, description = "The resource name to patch")
  ),
  responses(
    (status = 200, description = "The patched resource", body = Resource),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::patch("/resources/{name}")]
pub(crate) async fn put_resource(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ResourceUpdate>,
) -> HttpResult<web::HttpResponse> {
  let resource = ResourceDb::inspect_by_pk(&path.1, &state.pool).await?;
  let new_resource: ResourcePartial = ResourcePartial {
    name: path.1.clone(),
    version: payload.version.clone(),
    kind: resource.kind,
    data: payload.data.clone(),
    metadata: payload.metadata.clone(),
  };
  let resource = utils::resource::patch(&new_resource, &state).await?;
  Ok(web::HttpResponse::Ok().json(&resource))
}

/// List resource history
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources/{name}/histories",
  params(
    ("name" = String, Path, description = "The resource name to list history")
  ),
  responses(
    (status = 200, description = "The resource history", body = [ResourceSpec]),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::get("/resources/{name}/histories")]
pub(crate) async fn list_resource_history(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::new()
    .r#where("resource_key", GenericClause::Eq(path.1.clone()));
  let items = ResourceSpecDb::read(&filter, &state.pool)
    .await??
    .into_iter()
    .map(ResourceSpec::from)
    .collect::<Vec<_>>();
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Revert a resource to a specific history
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Resources",
  path = "/resources/{name}/histories/{id}/revert",
  params(
    ("name" = String, Path, description = "The resource name to revert"),
    ("id" = String, Path, description = "The resource history id to revert to")
  ),
  responses(
    (status = 200, description = "The resource has been revert", body = Resource),
    (status = 404, description = "Resource is not existing", body = ApiError),
  ),
))]
#[web::patch("/resources/{name}/histories/{id}/revert")]
pub(crate) async fn revert_resource(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let history = ResourceSpecDb::read_by_pk(&path.2, &state.pool).await??;
  let resource = ResourceDb::inspect_by_pk(&path.1, &state.pool).await?;
  let new_resource = ResourcePartial {
    name: resource.spec.resource_key,
    version: history.version,
    kind: resource.kind,
    data: history.data,
    metadata: history.metadata,
  };
  let resource = utils::resource::patch(&new_resource, &state).await?;
  Ok(web::HttpResponse::Ok().json(&resource))
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
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
  use ntex::http;
  use nanocl_stubs::{
    resource::{Resource, ResourcePartial, ResourceUpdate},
    generic::{GenericFilter, GenericClause, GenericListQuery},
  };

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/resources";

  #[ntex::test]
  async fn basic() {
    const TEST_RESOURCE: &str = "test_resource";
    let client = gen_default_test_client().await;
    let spec = serde_json::json!({
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
      name: TEST_RESOURCE.to_owned(),
      version: "v0.0.1".to_owned(),
      kind: "Kind".to_owned(),
      data: spec.clone(),
      metadata: Some(serde_json::json!({
        "Test": "gg",
      })),
    };
    let mut res = client
      .send_post(ENDPOINT, Some(&resource), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::CREATED,
      "create resource"
    );
    let resource = res.json::<Resource>().await.unwrap();
    assert_eq!(resource.spec.resource_key, TEST_RESOURCE);
    assert_eq!(resource.kind, String::from("Kind"));
    // Basic list
    let mut res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list resource");
    let _ = res.json::<Vec<Resource>>().await.unwrap();
    // Using filter exists
    let filter = GenericFilter::new()
      .r#where("data", GenericClause::HasKey("Schema".to_owned()));
    let query = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get(ENDPOINT, Some(&query)).await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "filter resource by data HasKey"
    );
    let resources = res.json::<Vec<Resource>>().await.unwrap();
    assert!(
      resources.len() == 1,
      "Expect 1 resource when filter by data HasKey"
    );
    let filter = GenericFilter::new().r#where(
      "data",
      GenericClause::Contains(serde_json::json!({
        "Schema": {
          "type": "object",
        }
      })),
    );
    let query = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get(ENDPOINT, Some(&query)).await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "filter resource by data contains"
    );
    let resources = res.json::<Vec<Resource>>().await.unwrap();
    assert!(
      resources.len() == 1,
      "Expect 1 resource when filter by data contains"
    );
    let filter = GenericFilter::new()
      .r#where("metadata", GenericClause::HasKey("Test".to_owned()));
    let query = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get(ENDPOINT, Some(&query)).await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "filter resource by meta exists"
    );
    let resources = res.json::<Vec<Resource>>().await.unwrap();
    assert!(
      resources.len() == 1,
      "Expect 1 resource when filter by metadata HasKey"
    );
    let filter = GenericFilter::new().r#where(
      "metadata",
      GenericClause::Contains(serde_json::json!({
        "Test": "gg",
      })),
    );
    let query = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get(ENDPOINT, Some(&query)).await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "filter resource by meta contains"
    );
    let resources = res.json::<Vec<Resource>>().await.unwrap();
    assert!(
      resources.len() == 1,
      "Expect 1 resource when filter by meta contains"
    );
    // Inspect
    let mut res = client
      .send_get(&format!("{ENDPOINT}/{TEST_RESOURCE}"), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "inspect resource");
    let resource = res.json::<Resource>().await.unwrap();
    assert_eq!(resource.spec.resource_key, TEST_RESOURCE);
    assert_eq!(resource.kind, String::from("Kind"));
    assert_eq!(&resource.spec.data, &spec);
    // History
    let _ = client
      .send_get(
        &format!("{ENDPOINT}/{TEST_RESOURCE}/histories"),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "list resource history"
    );
    let new_resource = ResourceUpdate {
      version: "v0.0.2".to_owned(),
      data: spec.clone(),
      metadata: None,
    };
    let mut res = client
      .send_patch(
        &format!("{ENDPOINT}/{TEST_RESOURCE}"),
        Some(&new_resource),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "patch resource");
    let resource = res.json::<Resource>().await.unwrap();
    assert_eq!(resource.spec.resource_key, TEST_RESOURCE);
    assert_eq!(resource.kind, String::from("Kind"));
    // Delete
    let resp = client
      .send_delete(&format!("{ENDPOINT}/{TEST_RESOURCE}"), None::<String>)
      .await;
    test_status_code!(
      resp.status(),
      http::StatusCode::ACCEPTED,
      "delete resource"
    );
  }
}
