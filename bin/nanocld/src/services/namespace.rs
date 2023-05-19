/*
* Endpoints to manipulate namespaces
*/
use ntex::web;

use nanocl_stubs::namespace::{NamespacePartial, NamespaceListQuery};

use crate::{utils, repositories};
use crate::models::DaemonState;

use nanocl_utils::http_error::HttpError;

/// List namespaces
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Namespaces",
  path = "/namespaces",
  params(
    ("Name" = Option<String>, Query, description = "Filter by name"),
    ("Limit" = Option<i64>, Query, description = "Limit the number of items returned"),
    ("Offset" = Option<i64>, Query, description = "Offset the number of items returned"),
  ),
  responses(
    (status = 200, description = "List of namespace", body = [NamespaceSummary]),
  ),
))]
#[web::get("/namespaces")]
pub(crate) async fn list_namespace(
  state: web::types::State<DaemonState>,
  web::types::Query(query): web::types::Query<NamespaceListQuery>,
) -> Result<web::HttpResponse, HttpError> {
  let items =
    utils::namespace::list(&query, &state.docker_api, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Get detailed information about a namespace
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Namespaces",
  path = "/namespaces/{Name}/inspect",
  params(
    ("Name" = String, Path, description = "The namespace name to inspect")
  ),
  responses(
    (status = 200, description = "Detailed information about a namespace", body = [NamespaceInspect]),
    (status = 404, description = "Namespace is not existing", body = ApiError),
  ),
))]
#[web::get("/namespaces/{name}/inspect")]
pub(crate) async fn inspect_namespace(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let namespace = utils::namespace::inspect(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&namespace))
}

/// Create a namespace
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = NamespacePartial,
  tag = "Namespaces",
  path = "/namespaces",
  responses(
    (status = 200, description = "List of namespace", body = Namespace),
    (status = 409, description = "Namespace already exist", body = ApiError),
  ),
))]
#[web::post("/namespaces")]
pub(crate) async fn create_namespace(
  web::types::Json(payload): web::types::Json<NamespacePartial>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let item = utils::namespace::create(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&item))
}

/// Delete a namespace
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Namespaces",
  path = "/namespaces/{Name}",
  params(
    ("Name" = String, Path, description = "The namespace name to delete")
  ),
  responses(
    (status = 200, description = "Delete response", body = GenericDelete),
    (status = 404, description = "Namespace is not existing", body = ApiError),
  ),
))]
#[web::delete("/namespaces/{name}")]
pub(crate) async fn delete_namespace(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  repositories::namespace::find_by_name(&path.1, &state.pool).await?;
  let res = utils::namespace::delete_by_name(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&res))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_namespace);
  config.service(create_namespace);
  config.service(inspect_namespace);
  config.service(delete_namespace);
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
