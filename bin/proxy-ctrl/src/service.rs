use ntex::web;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::resource::ResourcePartial;

use crate::{error::HttpError, utils, nginx::Nginx};

#[web::post("/rules")]
async fn apply_rule(
  web::types::Json(payload): web::types::Json<ResourcePartial>,
  nginx: web::types::State<Nginx>,
) -> Result<web::HttpResponse, HttpError> {
  let client = NanocldClient::connect_with_unix_default();

  utils::create_resource_conf(&client, &nginx, &payload).await?;
  utils::reload_config(&client).await?;

  Ok(web::HttpResponse::Ok().finish())
}

#[web::delete("/rules/{name}")]
async fn remove_rule(name: web::types::Path<String>) -> web::HttpResponse {
  println!("received payload: {name:?}");

  web::HttpResponse::Ok().finish()
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(apply_rule);
}

#[cfg(test)]
mod tests {
  use super::*;

  use ntex::http::StatusCode;
  use nanocld_client::stubs::resource::ResourcePartial;

  use crate::utils::tests;

  #[ntex::test]
  async fn rule() {
    let test_srv = tests::generate_server(ntex_config);

    let resource: &str = include_str!("../tests/resource_redirect.yml");

    let yaml: serde_yaml::Value = serde_yaml::from_str(resource).unwrap();

    let resource = yaml["Resources"][0].clone();

    let resource = serde_yaml::from_value::<ResourcePartial>(resource).unwrap();

    let res = test_srv.post("/rules").send_json(&resource).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
  }
}
