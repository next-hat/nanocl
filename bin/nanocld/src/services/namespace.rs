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
  let namespace = utils::namespace::inspect_by_name(&path.1, &state).await?;
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
  use serde_json::json;

  use nanocl_stubs::generic::GenericDelete;
  use nanocl_stubs::namespace::{Namespace, NamespacePartial};

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/namespaces";

  async fn list(client: &TestClient) {
    let res = client.send_get(ENDPOINT, None::<String>).await;
    assert!(res.status().is_success(), "Expect success on list");
    let _ = TestClient::res_json::<Vec<Namespace>>(res).await;
  }

  async fn create(client: &TestClient) {
    let new_namespace = NamespacePartial {
      name: String::from("controller-default"),
    };
    let res = client
      .send_post(ENDPOINT, Some(new_namespace), None::<String>)
      .await;
    assert!(res.status().is_success(), "Expect success on create");
  }

  async fn test_fail_create(client: &TestClient) {
    let res = client
      .send_post(
        ENDPOINT,
        Some(&json!({
          "name": 1,
        })),
        None::<String>,
      )
      .await;
    assert!(
      res.status().is_client_error(),
      "Expect error for invalid body"
    );
    let res = client
      .send_post(ENDPOINT, None::<String>, None::<String>)
      .await;
    assert!(res.status().is_client_error(), "Expect error when no body");
  }

  async fn inspect_by_id(client: &TestClient) {
    const NAME: &str = "controller-default";
    let res = client
      .send_get(&format!("{ENDPOINT}/{NAME}/inspect"), None::<String>)
      .await;
    assert!(res.status().is_success(), "Expect success on inspect_by_id");
  }

  async fn delete(client: &TestClient) {
    const NAME: &str = "controller-default";
    let res = client
      .send_delete(&format!("{ENDPOINT}/{NAME}"), None::<String>)
      .await;
    assert!(res.status().is_success(), "Expect success on delete");
    let body = TestClient::res_json::<GenericDelete>(res).await;
    assert_eq!(body.count, 1, "Expect 1 item deleted");
  }

  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    test_fail_create(&client).await;
    create(&client).await;
    inspect_by_id(&client).await;
    list(&client).await;
    delete(&client).await;
  }
}
