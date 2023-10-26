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
    (status = 200, description = "The created rule", body = ResourceProxyRule),
  ),
))]
#[web::put("/rules/{name}")]
pub async fn apply_rule(
  path: web::types::Path<(String, String)>,
  nginx: web::types::State<Nginx>,
  web::types::Json(payload): web::types::Json<ResourceProxyRule>,
) -> Result<web::HttpResponse, HttpError> {
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
  }

  utils::create_resource_conf(&path.1, &payload, &client, &nginx).await?;
  if let Err(err) = utils::reload_config(&client).await {
    nginx.delete_conf_file(&path.1).await;
    utils::reload_config(&client).await?;
    return Err(HttpError::bad_request(err.to_string()));
  }

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
    (status = 200, description = "Rule has been deleted"),
  ),
))]
#[web::delete("/rules/{name}")]
pub async fn remove_rule(
  path: web::types::Path<(String, String)>,
  nginx: web::types::State<Nginx>,
) -> Result<web::HttpResponse, HttpError> {
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
  }
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

  use crate::utils::tests;

  #[ntex::test]
  async fn rules() {
    let test_srv = tests::generate_server();

    let resource: &str = include_str!("../../tests/resource_redirect.yml");

    let yaml: serde_yaml::Value = serde_yaml::from_str(resource).unwrap();

    let resource = yaml["Resources"][0].clone();
    let name = resource["Name"].as_str().unwrap();

    let payload = resource["Config"].clone();

    let mut res = test_srv
      .put(format!("/v0.4/rules/{name}"))
      .send_json(&payload)
      .await
      .unwrap();

    println!("{:#?}", res);
    let json = res.json::<serde_json::Value>().await.unwrap();
    println!("{:#?}", json);
    assert_eq!(res.status(), http::StatusCode::OK);

    let res = test_srv
      .delete(format!("/v0.4/rules/{}", name))
      .send()
      .await
      .unwrap();

    assert_eq!(res.status(), http::StatusCode::OK);
  }
}
