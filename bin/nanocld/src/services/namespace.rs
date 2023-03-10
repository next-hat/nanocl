/*
* Endpoints to manipulate namespaces
*/
use ntex::web;

use nanocl_stubs::namespace::NamespacePartial;

use crate::utils;
use crate::models::Pool;

use crate::error::HttpResponseError;

#[web::get("/namespaces")]
pub(crate) async fn list_namespace(
  pool: web::types::State<Pool>,
  docker_api: web::types::State<bollard_next::Docker>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let items = utils::namespace::list(&docker_api, &pool).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Endpoint to create new namespace
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  path = "/namespaces",
  request_body = NamespacePartial,
  responses(
    (status = 201, description = "fresh created namespace", body = NamespaceItem),
    (status = 400, description = "generic database error", body = ApiError),
    (status = 422, description = "the provided payload is not valid", body = ApiError),
  ),
))]
#[web::post("/namespaces")]
async fn create_namespace(
  web::types::Json(payload): web::types::Json<NamespacePartial>,
  docker_api: web::types::State<bollard_next::Docker>,
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Creating namespace: {:?}", &payload);
  let item = utils::namespace::create(&payload, &docker_api, &pool).await?;
  log::debug!("Namespace created: {:?}", &item);
  Ok(web::HttpResponse::Created().json(&item))
}

#[web::delete("/namespaces/{name}")]
async fn delete_namespace_by_name(
  path: web::types::Path<(String, String)>,
  docker_api: web::types::State<bollard_next::Docker>,
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Deleting namespace {}", &path.1);
  let res =
    utils::namespace::delete_by_name(&path.1, &docker_api, &pool).await?;
  log::debug!("Namespace {} deleted: {:?}", &path.1, &res);
  Ok(web::HttpResponse::Ok().json(&res))
}

#[web::get("/namespaces/{id}/inspect")]
async fn inspect_namespace_by_name(
  path: web::types::Path<(String, String)>,
  docker_api: web::types::State<bollard_next::Docker>,
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  log::debug!("Inspecting namespace {}", path.1);
  let namespace =
    utils::namespace::inspect(&path.1, &docker_api, &pool).await?;
  log::debug!("Namespace found: {:?}", &namespace);
  Ok(web::HttpResponse::Ok().json(&namespace))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_namespace);
  config.service(create_namespace);
  config.service(inspect_namespace_by_name);
  config.service(delete_namespace_by_name);
}

#[cfg(test)]
mod test_namespace {
  use crate::services::ntex_config;

  use serde_json::json;

  use nanocl_stubs::namespace::NamespacePartial;
  use nanocl_stubs::generic::GenericDelete;

  use crate::utils::tests::*;

  async fn test_list(srv: &TestServer) -> TestRet {
    let resp = srv.get("/v0.2/namespaces").send().await?;

    assert!(resp.status().is_success());
    Ok(())
  }

  async fn test_create(srv: &TestServer) -> TestRet {
    let new_namespace = NamespacePartial {
      name: String::from("controller-default"),
    };

    let resp = srv
      .post("/v0.2/namespaces")
      .send_json(&new_namespace)
      .await?;

    assert!(resp.status().is_success());
    Ok(())
  }

  async fn test_fail_create(srv: &TestServer) -> TestRet {
    let resp = srv
      .post("/v0.2/namespaces")
      .send_json(&json!({
          "name": 1,
      }))
      .await?;

    assert!(resp.status().is_client_error());

    let resp = srv.post("/v0.2/namespaces").send().await?;

    assert!(resp.status().is_client_error());
    Ok(())
  }

  async fn test_inspect_by_id(srv: &TestServer) -> TestRet {
    let resp = srv
      .get(format!(
        "/v0.2/namespaces/{name}/inspect",
        name = "controller-default"
      ))
      .send()
      .await?;

    assert!(resp.status().is_success());
    Ok(())
  }

  async fn test_delete(srv: &TestServer) -> TestRet {
    let mut resp = srv
      .delete(format!(
        "/v0.2/namespaces/{name}",
        name = "controller-default"
      ))
      .send()
      .await?;

    let body = resp.json::<GenericDelete>().await?;
    assert_eq!(body.count, 1);
    assert!(resp.status().is_success());
    Ok(())
  }

  #[ntex::test]
  async fn basic() -> TestRet {
    let srv = generate_server(ntex_config).await;

    test_fail_create(&srv).await?;
    test_create(&srv).await?;
    test_inspect_by_id(&srv).await?;
    test_list(&srv).await?;
    test_delete(&srv).await?;
    Ok(())
  }
}
