use ntex::web;

use nanocl_error::http::{HttpResult, HttpError};

use nanocl_stubs::{
  generic::{GenericFilter, GenericListQuery},
  namespace::NamespacePartial,
};

use crate::{
  objects::generic::*,
  models::{SystemState, NamespaceDb},
};

/// List namespaces
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Namespaces",
  path = "/namespaces",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"where\": { \"name\": { \"eq\": \"test\" } } }"),
  ),
  responses(
    (status = 200, description = "List of namespace", body = [NamespaceSummary]),
  ),
))]
#[web::get("/namespaces")]
pub async fn list_namespace(
  state: web::types::State<SystemState>,
  query: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::try_from(query.into_inner())
    .map_err(|err| HttpError::bad_request(err.to_string()))?;
  let items = NamespaceDb::list(&filter, &state).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Get detailed information about a namespace
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Namespaces",
  path = "/namespaces/{name}/inspect",
  params(
    ("name" = String, Path, description = "The namespace name to inspect")
  ),
  responses(
    (status = 200, description = "Detailed information about a namespace", body = [NamespaceInspect]),
    (status = 404, description = "Namespace is not existing", body = ApiError),
  ),
))]
#[web::get("/namespaces/{name}/inspect")]
pub async fn inspect_namespace(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let namespace = NamespaceDb::inspect_obj_by_pk(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&namespace))
}

/// Create a namespace
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = NamespacePartial,
  tag = "Namespaces",
  path = "/namespaces",
  responses(
    (status = 200, description = "The created namespace", body = Namespace),
    (status = 409, description = "Namespace already exist", body = ApiError),
  ),
))]
#[web::post("/namespaces")]
pub async fn create_namespace(
  state: web::types::State<SystemState>,
  payload: web::types::Json<NamespacePartial>,
) -> HttpResult<web::HttpResponse> {
  let item = NamespaceDb::create_obj(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&item))
}

/// Delete a namespace
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Namespaces",
  path = "/namespaces/{name}",
  params(
    ("name" = String, Path, description = "Name of the namespace to delete")
  ),
  responses(
    (status = 202, description = "Namespace have been deleted"),
    (status = 404, description = "Namespace is not existing", body = ApiError),
  ),
))]
#[web::delete("/namespaces/{name}")]
pub async fn delete_namespace(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  NamespaceDb::del_obj_by_pk(&path.1, &(), &state).await?;
  Ok(web::HttpResponse::Accepted().into())
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
  }

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    test_fail_create(&client).await;
    create(&client).await;
    inspect_by_id(&client).await;
    list(&client).await;
    delete(&client).await;
    system.state.wait_event_loop().await;
  }
}
