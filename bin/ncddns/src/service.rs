use ntex::web;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::dns::ResourceDnsRule;
use nanocl_utils::http_error::HttpError;

use crate::utils;
use crate::dnsmasq::Dnsmasq;

#[web::put("/rules/{name}")]
async fn dns_entry(
  name: web::types::Path<String>,
  dnsmasq: web::types::State<Dnsmasq>,
  web::types::Json(payload): web::types::Json<ResourceDnsRule>,
) -> Result<web::HttpResponse, HttpError> {
  let client = NanocldClient::connect_with_unix_default();
  utils::write_rule(&name, &payload, &dnsmasq, &client).await?;
  Ok(web::HttpResponse::Ok().json(&payload))
}

#[web::delete("/rules/{name}")]
async fn dns_entry_delete(
  path: web::types::Path<String>,
  dnsmasq: web::types::State<Dnsmasq>,
) -> Result<web::HttpResponse, HttpError> {
  dnsmasq.remove_config(&path).await?;

  Ok(web::HttpResponse::Ok().finish())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(dns_entry);
  config.service(dns_entry_delete);
}

#[cfg(test)]
mod tests {
  use super::*;

  use ntex::http::StatusCode;

  use crate::utils::tests;

  #[ntex::test]
  async fn rules() {
    let test_srv = tests::generate_server(ntex_config);

    let resource: &str = include_str!("../tests/resource_dns.yml");

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
      .delete(format!("/rules/{name}"))
      .send()
      .await
      .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
  }
}
