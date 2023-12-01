use ntex::web;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::proxy::ResourceProxyRule;
use nanocl_error::http::HttpError;

use crate::utils;
use crate::nginx::Nginx;

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
  client: web::types::State<NanocldClient>,
  nginx: web::types::State<Nginx>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ResourceProxyRule>,
) -> Result<web::HttpResponse, HttpError> {
  utils::create_resource_conf(&path.1, &payload, &client, &nginx).await?;
  if let Err(err) = utils::reload_config(&client).await {
    nginx.delete_conf_file(&path.1).await;
    utils::reload_config(&client).await?;
    return Err(HttpError::bad_request(err.to_string()));
  }
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
  client: web::types::State<NanocldClient>,
  nginx: web::types::State<Nginx>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpError> {
  nginx.delete_conf_file(&path.1).await;
  utils::reload_config(&client).await?;
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
  async fn rules() {
    let client = gen_default_test_client().await;
    let resource: &str = include_str!("../../tests/resource_redirect.yml");
    let yaml: serde_yaml::Value = serde_yaml::from_str(resource).unwrap();
    let resource = yaml["Resources"][0].clone();
    let name = resource["Name"].as_str().unwrap();
    let payload = resource["Data"].clone();
    let res = client
      .send_put(&format!("/rules/{name}"), Some(&payload), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "put a rule");
    let res = client
      .send_delete(&format!("/rules/{name}"), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "delete a rule");
  }
}
