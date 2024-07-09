use ntex::web;

use nanocl_error::http::HttpError;

use nanocld_client::stubs::proxy::ResourceProxyRule;

use crate::{models::SystemStateRef, utils};

/// Create/Update a new ProxyRule
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Rules",
  path = "/rules/{name}",
  request_body = ResourceProxyRule,
  params(
    ("name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "The created rule", body = ResourceProxyRule),
  ),
))]
#[web::put("/rules/{name}")]
pub async fn apply_rule(
  state: web::types::State<SystemStateRef>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ResourceProxyRule>,
) -> Result<web::HttpResponse, HttpError> {
  log::info!("apply_rule: {}", path.1);
  utils::nginx::add_rule(&path.1, &payload, &state).await?;
  state.event_emitter.emit_reload().await;
  Ok(web::HttpResponse::Ok().json(&payload.into_inner()))
}

/// Delete a ProxyRule
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Rules",
  path = "/rules/{name}",
  params(
    ("name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "Rule has been deleted"),
  ),
))]
#[web::delete("/rules/{name}")]
pub async fn remove_rule(
  state: web::types::State<SystemStateRef>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpError> {
  log::info!("remove_rule: {}", path.1);
  utils::nginx::del_rule(&path.1, &state).await;
  state.event_emitter.emit_reload().await;
  Ok(web::HttpResponse::Ok().finish())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(apply_rule);
  config.service(remove_rule);
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() {
    let name = "ncproxy-io-test-resource";
    let client = gen_default_test_client().await;
    ensure_test_cargo().await.unwrap();
    let payload = read_rule("tests/basic.yml").unwrap();
    let mut res = client
      .send_put(&format!("/rules/{name}"), Some(&payload), None::<String>)
      .await;
    let json = res.json::<serde_yaml::Value>().await.unwrap();
    println!("{:?}", json);
    test_status_code!(res.status(), http::StatusCode::OK, "put a rule");
    let res = client
      .send_delete(&format!("/rules/{name}"), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "delete a rule");
    clean_test_cargo().await.unwrap();
  }
}
