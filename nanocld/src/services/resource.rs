/*
* Endpoints to manipulate cargoes
*/
use std::sync::{Mutex, Arc};

use ntex::rt;
use ntex::web;

use nanocl_models::system::Event;
use nanocl_models::generic::GenericNspQuery;

use crate::models::ResourcePartial;
use crate::repositories;
use crate::event::EventEmitter;
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
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Creating resource: {:?}", &payload);
  let resource = repositories::resource::create(payload, &pool).await?;
  log::debug!("Resource created: {:?}", &resource);
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
) -> Result<web::HttpResponse, HttpResponseError> {
  let key = name.into_inner();
  log::debug!("Deleting resource: {}", &key);
  repositories::resource::delete_by_key(key.to_owned(), &pool).await?;
  repositories::resource::delete_config_by_resource_key(key, &pool).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_resource);
  config.service(delete_resource);
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  use crate::{
    utils::tests::*,
    models::{ResourceKind, Resource},
  };

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
    assert_eq!(resp.status(), 201);
    let resource = resp.json::<Resource>().await.unwrap();
    assert_eq!(resource.name, "test_resource");
    assert_eq!(resource.kind, ResourceKind::ProxyRule);

    // Delete
    let resp = srv.delete("/resources/test_resource").send().await.unwrap();
    assert_eq!(resp.status(), 202);
    Ok(())
  }
}
