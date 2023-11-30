use nanocl_stubs::generic::GenericFilter;
/*
* Endpoints to manipulate secrets
*/
use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::proxy::ProxySslConfig;
use nanocl_stubs::secret::{SecretPartial, SecretUpdate, SecretQuery};

use crate::utils;
use crate::models::{DaemonState, SecretDb, Repository};

/// List secrets
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Secrets",
  path = "/secrets",
  responses(
    (status = 200, description = "List of secret", body = [Secret]),
  ),
))]
#[web::get("/secrets")]
pub(crate) async fn list_secret(
  state: web::types::State<DaemonState>,
  web::types::Query(query): web::types::Query<SecretQuery>,
) -> HttpResult<web::HttpResponse> {
  let items = SecretDb::find(&GenericFilter::default(), &state.pool).await??;
  Ok(web::HttpResponse::Ok().json(&items))
}

/// Get detailed information about a secret
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Secrets",
  path = "/secrets/{Key}/inspect",
  params(
    ("Name" = String, Path, description = "The secret name to inspect")
  ),
  responses(
    (status = 200, description = "Detailed information about a secret", body = [Secret]),
    (status = 404, description = "Namespace is not existing", body = ApiError),
  ),
))]
#[web::get("/secrets/{key}/inspect")]
pub(crate) async fn inspect_secret(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> HttpResult<web::HttpResponse> {
  let secret = SecretDb::find_by_pk(&path.1, &state.pool).await??;
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
  web::types::Json(payload): web::types::Json<SecretPartial>,
  state: web::types::State<DaemonState>,
) -> HttpResult<web::HttpResponse> {
  match payload.kind.as_str() {
    "Tls" => {
      serde_json::from_value::<ProxySslConfig>(payload.data.clone()).map_err(
        |e| {
          HttpError::bad_request(format!(
            "Invalid data for secret of kind Tls: {e}",
          ))
        },
      )?;
    }
    "Env" => {
      serde_json::from_value::<Vec<String>>(payload.data.clone()).map_err(
        |e| {
          HttpError::bad_request(format!(
            "Invalid data for secret of kind Env: {e}",
          ))
        },
      )?;
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
  path = "/secrets/{Key}",
  params(
    ("Name" = String, Path, description = "The secret name to delete")
  ),
  responses(
    (status = 200, description = "Delete response", body = GenericDelete),
    (status = 404, description = "Namespace is not existing", body = ApiError),
  ),
))]
#[web::delete("/secrets/{key}")]
pub(crate) async fn delete_secret(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> HttpResult<web::HttpResponse> {
  let res = utils::secret::delete_by_key(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&res))
}

/// Scale or Downscale number of instances
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Secrets",
  request_body = SecretUpdate,
  path = "/secrets/{Key}",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Secret scaled", body = Secret),
    (status = 404, description = "Secret does not exist", body = ApiError),
  ),
))]
#[web::patch("/secrets/{key}")]
async fn patch_secret(
  web::types::Json(payload): web::types::Json<SecretUpdate>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> HttpResult<web::HttpResponse> {
  let item = utils::secret::patch_by_key(&path.1, &payload, &state).await?;
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

  use nanocl_stubs::secret::{Secret, SecretPartial, SecretQuery};
  use nanocl_stubs::generic::GenericDelete;

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/secrets";

  async fn test_list(client: &TestClient) {
    let res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list secrets");
    let res = client
      .send_get(
        ENDPOINT,
        Some(SecretQuery {
          kind: Some("Tls".to_owned()),
          contains: Some(serde_json::json!({"VerifyClient": true}).to_string()),
          exists: Some("CertificateClient".to_owned()),
          meta_contains: Some(
            serde_json::json!({"CertManagerIssuer": "letsencrypt"}).to_string(),
          ),
          meta_exists: Some("cert_manager_domain".to_owned()),
        }),
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "list secrets");
  }

  async fn test_create(client: &TestClient) {
    let new_secret = SecretPartial {
      key: String::from("test-secret"),
      kind: String::from("test"),
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
    test_status_code!(res.status(), http::StatusCode::OK, "delete secret");
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
