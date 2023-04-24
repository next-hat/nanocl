use ntex::web;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::resource::ResourcePartial;

use nanocl_utils::http_error::HttpError;

use crate::utils;
use crate::nginx::{Nginx, NginxConfKind};

#[web::put("/rules")]
async fn apply_rule(
  nginx: web::types::State<Nginx>,
  web::types::Json(payload): web::types::Json<ResourcePartial>,
) -> Result<web::HttpResponse, HttpError> {
  let client = NanocldClient::connect_with_unix_default();

  utils::create_resource_conf(&client, &nginx, &payload).await?;
  utils::reload_config(&client).await?;

  Ok(web::HttpResponse::Ok().json(&payload))
}

#[web::delete("/rules/{kind}/{name}")]
async fn remove_rule(
  path: web::types::Path<(String, String)>,
  nginx: web::types::State<Nginx>,
) -> Result<web::HttpResponse, HttpError> {
  let (kind, name) = path.into_inner();

  println!("Deleting rule: {kind} {name}");

  let kind: NginxConfKind = kind.parse()?;

  if let Err(err) = nginx.delete_conf_file(&name, &kind).await {
    log::warn!("Failed to delete file: {}", err);
  }

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
  use nanocld_client::stubs::resource::ResourcePartial;

  use crate::utils::tests;

  #[ntex::test]
  async fn rules() {
    let test_srv = tests::generate_server(ntex_config);

    let resource: &str = include_str!("../tests/resource_redirect.yml");

    let yaml: serde_yaml::Value = serde_yaml::from_str(resource).unwrap();

    let resource = yaml["Resources"][0].clone();

    let resource = serde_yaml::from_value::<ResourcePartial>(resource).unwrap();

    let res = test_srv.put("/rules").send_json(&resource).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let res = test_srv
      .delete(format!("/rules/site/{}", resource.name))
      .send()
      .await
      .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
  }
}
