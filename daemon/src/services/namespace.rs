/// Manage nanocl namespace
use ntex::web;

use crate::repositories;
use crate::models::{Pool, NamespacePartial};

use crate::error::HttpResponseError;

/// List all namespace
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  path = "/namespaces",
  responses(
      (status = 200, description = "Array of namespace", body = [NamespaceItem]),
  ),
))]
#[web::get("/namespaces")]
async fn list_namespace(
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let items = repositories::namespace::list(&pool).await?;

  Ok(web::HttpResponse::Ok().json(&items))
}

/// Create new namespace
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
  pool: web::types::State<Pool>,
  web::types::Json(payload): web::types::Json<NamespacePartial>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let item = repositories::namespace::create(payload, &pool).await?;

  Ok(web::HttpResponse::Created().json(&item))
}

/// Delete namespace by it's name
#[cfg_attr(feature = "dev", utoipa::path(
    delete,
    path = "/namespaces/{name}",
    responses(
        (status = 200, description = "database generic delete response", body = GenericDelete),
    ),
    params(
        ("name" = String, Path, description = "name of the namespace"),
    )
))]
#[web::delete("/namespaces/{name}")]
async fn delete_namespace_by_name(
  pool: web::types::State<Pool>,
  id: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let id_or_name = id.into_inner();
  let res = repositories::namespace::delete_by_name(id_or_name, &pool).await?;
  Ok(web::HttpResponse::Ok().json(&res))
}

/// Inspect namespace by it's name
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  path = "/namespaces/{name}/inspect",
  responses(
      (status = 200, description = "Namespace found", body = NamespaceItem),
      (status = 404, description = "Namespace not found", body = ApiError),
  ),
  params(
    ("name" = String, Path, description = "name of the namespace"),
  )
))]
#[web::get("/namespaces/{id}/inspect")]
async fn inspect_namespace_by_name(
  name: web::types::Path<String>,
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = name.into_inner();
  let item = repositories::namespace::inspect_by_name(name, &pool).await?;

  let _cargoes =
    repositories::cargo::find_by_namespace(item.to_owned(), &pool).await?;

  Ok(web::HttpResponse::Ok().json(&item))
}

/// # ntex config
/// Bind namespace routes to ntex http server
///
/// # Arguments
/// [config](web::ServiceConfig) mutable service config
///
/// # Examples
/// ```rust,norun
/// use ntex::web;
/// use crate::controllers;
///
/// web::App::new().configure(controllers::namespace::ntex_config)
/// ```
pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_namespace);
  config.service(create_namespace);
  config.service(inspect_namespace_by_name);
  config.service(delete_namespace_by_name);
}

#[cfg(test)]
mod test_namespace {
  use super::*;

  use serde_json::json;

  use nanocl_models::generic::GenericDelete;

  use crate::models::NamespacePartial;
  use crate::utils::tests::*;

  async fn test_list(srv: &TestServer) -> TestRet {
    let resp = srv.get("/namespaces").send().await?;

    assert!(resp.status().is_success());
    Ok(())
  }

  async fn test_create(srv: &TestServer) -> TestRet {
    let new_namespace = NamespacePartial {
      name: String::from("controller-default"),
    };

    let resp = srv.post("/namespaces").send_json(&new_namespace).await?;

    assert!(resp.status().is_success());
    Ok(())
  }

  async fn test_fail_create(srv: &TestServer) -> TestRet {
    let resp = srv
      .post("/namespaces")
      .send_json(&json!({
          "name": 1,
      }))
      .await?;

    assert!(resp.status().is_client_error());

    let resp = srv.post("/namespaces").send().await?;

    assert!(resp.status().is_client_error());
    Ok(())
  }

  async fn test_inspect_by_id(srv: &TestServer) -> TestRet {
    let resp = srv
      .get(format!(
        "/namespaces/{name}/inspect",
        name = "controller-default"
      ))
      .send()
      .await?;

    assert!(resp.status().is_success());
    Ok(())
  }

  async fn test_delete(srv: &TestServer) -> TestRet {
    let mut resp = srv
      .delete(format!("/namespaces/{name}", name = "controller-default"))
      .send()
      .await?;

    let body = resp.json::<GenericDelete>().await?;
    assert_eq!(body.count, 1);
    assert!(resp.status().is_success());
    Ok(())
  }

  #[ntex::test]
  async fn main() -> TestRet {
    let srv = generate_server(ntex_config).await;

    test_fail_create(&srv).await?;
    test_create(&srv).await?;
    test_inspect_by_id(&srv).await?;
    test_list(&srv).await?;
    test_delete(&srv).await?;
    Ok(())
  }
}
