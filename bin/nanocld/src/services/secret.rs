/*
* Endpoints to manipulate secrets
*/
use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::{
  generic::{GenericFilter, GenericListQuery},
  secret::{SecretPartial, SecretUpdate},
  proxy::ProxySslConfig,
};

use crate::{
  utils,
  repositories::generic::*,
  models::{DaemonState, SecretDb},
};

/// List secret
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Secrets",
  path = "/secrets",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"where\": { \"kind\": { \"eq\": \"Env\" } } }"),
  ),
  responses(
    (status = 200, description = "List of secret", body = [Secret]),
  ),
))]
#[web::get("/secrets")]
pub(crate) async fn list_secret(
  state: web::types::State<DaemonState>,
  query: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::try_from(query.into_inner())
    .map_err(|err| HttpError::bad_request(err.to_string()))?;
  let items = SecretDb::transform_read_by(&filter, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Get detailed information about a secret
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Secrets",
  path = "/secrets/{key}/inspect",
  params(
    ("key" = String, Path, description = "Key of the secret")
  ),
  responses(
    (status = 200, description = "Detailed information about a secret", body = Secret),
    (status = 404, description = "Namespace is not existing", body = ApiError),
  ),
))]
#[web::get("/secrets/{key}/inspect")]
pub(crate) async fn inspect_secret(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let secret = SecretDb::transform_read_by_pk(&path.1, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&secret))
}

/// Create a secret
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = SecretPartial,
  tag = "Secrets",
  path = "/secrets",
  responses(
    (status = 200, description = "List of secret", body = Secret),
    (status = 409, description = "Namespace already exist", body = ApiError),
  ),
))]
#[web::post("/secrets")]
pub(crate) async fn create_secret(
  state: web::types::State<DaemonState>,
  payload: web::types::Json<SecretPartial>,
) -> HttpResult<web::HttpResponse> {
  utils::key::ensure_kind(&payload.kind)?;
  match payload.kind.as_str() {
    "nanocl.io/tls" => {
      serde_json::from_value::<ProxySslConfig>(payload.data.clone())
        .map_err(|e| HttpError::bad_request(e.to_string()))?;
    }
    "nanocl.io/env" => {
      serde_json::from_value::<Vec<String>>(payload.data.clone())
        .map_err(|e| HttpError::bad_request(e.to_string()))?;
    }
    _ => {}
  }
  let item = utils::secret::create(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&item))
}

/// Delete a secret
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Secrets",
  path = "/secrets/{key}",
  params(
    ("key" = String, Path, description = "Key of the secret")
  ),
  responses(
    (status = 202, description = "Secret have been deleted"),
    (status = 404, description = "Secret don't exists", body = ApiError),
  ),
))]
#[web::delete("/secrets/{key}")]
pub(crate) async fn delete_secret(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  utils::secret::delete_by_pk(&path.1, &state).await?;
  Ok(web::HttpResponse::Accepted().into())
}

/// Update a secret
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Secrets",
  request_body = SecretUpdate,
  path = "/secrets/{key}",
  params(
    ("key" = String, Path, description = "Key of the secret"),
  ),
  responses(
    (status = 200, description = "Secret scaled", body = Secret),
    (status = 404, description = "Secret does not exist", body = ApiError),
  ),
))]
#[web::patch("/secrets/{key}")]
async fn patch_secret(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<SecretUpdate>,
) -> HttpResult<web::HttpResponse> {
  let item = utils::secret::patch_by_pk(&path.1, &payload, &state).await?;
  Ok(web::HttpResponse::Ok().json(&item))
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_secret);
  config.service(create_secret);
  config.service(inspect_secret);
  config.service(delete_secret);
  config.service(patch_secret);
}

#[cfg(test)]
mod test_secret {
  use ntex::http;

  use serde_json::json;

  use nanocl_stubs::secret::{Secret, SecretPartial};

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/secrets";

  async fn test_list(client: &TestClient) {
    let res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list secrets");
  }

  async fn test_create(client: &TestClient) {
    let new_secret = SecretPartial {
      name: String::from("test-secret"),
      kind: String::from("test-create.io/test"),
      immutable: None,
      data: json!({
        "Tls": { "cert": "MY CERT", "key": "MY KEY" },
      }),
      metadata: None,
    };
    let mut res = client
      .send_post(ENDPOINT, Some(new_secret), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::CREATED, "create secret");
    let _ = res.json::<Secret>().await.unwrap();
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
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "create secret with invalid body"
    );
    let res = client
      .send_post(ENDPOINT, None::<String>, None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "create secret with no body"
    );
  }

  async fn test_inspect_by_id(client: &TestClient) {
    let res = client
      .send_get(&format!("{ENDPOINT}/test-secret/inspect"), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "inspect secret");
  }

  async fn test_delete(client: &TestClient) {
    let res = client
      .send_delete(&format!("{ENDPOINT}/test-secret"), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "delete secret"
    );
  }

  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    test_fail_create(&client).await;
    test_create(&client).await;
    test_inspect_by_id(&client).await;
    test_list(&client).await;
    test_delete(&client).await;
  }
}
