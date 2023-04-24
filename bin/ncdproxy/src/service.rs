use ntex::web;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::proxy::ResourceProxyRule;

use nanocl_utils::http_error::HttpError;

use crate::utils;
use crate::nginx::Nginx;

/// Create/Update a new ProxyRule
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Rules",
  path = "/rules/{Name}",
  request_body = ResourceProxyRule,
  params(
    ("Name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "List of namespace", body = ResourceProxyRule),
  ),
))]
#[web::put("/rules/{name}")]
async fn apply_rule(
  name: web::types::Path<String>,
  nginx: web::types::State<Nginx>,
  web::types::Json(payload): web::types::Json<ResourceProxyRule>,
) -> Result<web::HttpResponse, HttpError> {
  let client = NanocldClient::connect_with_unix_default();

  utils::create_resource_conf(&name, &payload, &client, &nginx).await?;
  utils::reload_config(&client).await?;

  Ok(web::HttpResponse::Ok().json(&payload))
}

/// Delete a ProxyRule
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Rules",
  path = "/rules/{Name}",
  params(
    ("Name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "List of namespace", body = ResourceProxyRule),
  ),
))]
#[web::delete("/rules/{name}")]
async fn remove_rule(
  name: web::types::Path<String>,
  nginx: web::types::State<Nginx>,
) -> Result<web::HttpResponse, HttpError> {
  nginx.delete_conf_file(&name).await;

  Ok(web::HttpResponse::Ok().finish())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(apply_rule);
  config.service(remove_rule);
}

#[cfg(test)]
mod tests {
  use super::*;

  use ntex::http::StatusCode;

  use crate::utils::tests;

  #[ntex::test]
  async fn rules() {
    let test_srv = tests::generate_server(ntex_config);

    let resource: &str = include_str!("../tests/resource_redirect.yml");

    let yaml: serde_yaml::Value = serde_yaml::from_str(resource).unwrap();

    let resource = yaml["Resources"][0].clone();
    let name = resource["Name"].as_str().unwrap();

    let res = test_srv
      .put(format!("/rules/{name}"))
      .send_json(&resource["Config"])
      .await
      .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let res = test_srv
      .delete(format!("/rules/{}", name))
      .send()
      .await
      .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
  }
}
