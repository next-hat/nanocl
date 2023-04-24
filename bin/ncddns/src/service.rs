use ntex::web;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::dns::ResourceDnsRule;
use nanocld_client::stubs::resource::ResourcePartial;

use nanocl_utils::io_error::FromIo;
use nanocl_utils::http_error::HttpError;

use crate::utils;
use crate::dnsmasq::Dnsmasq;

#[web::put("/rules")]
async fn dns_entry(
  dnsmasq: web::types::State<Dnsmasq>,
  web::types::Json(payload): web::types::Json<ResourcePartial>,
) -> Result<web::HttpResponse, HttpError> {
  let client = NanocldClient::connect_with_unix_default();
  let dns_rule = serde_json::from_value::<ResourceDnsRule>(payload.config)
    .map_err(|err| err.map_err_context(|| "unable to parse config"))?;
  utils::write_rule(&payload.name, &dns_rule, &dnsmasq, &client).await?;
  Ok(web::HttpResponse::Ok().finish())
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
  use nanocld_client::stubs::resource::ResourcePartial;

  use crate::utils::tests;

  #[ntex::test]
  async fn rules() {
    let test_srv = tests::generate_server(ntex_config);

    let resource: &str = include_str!("../tests/resource_dns.yml");

    let yaml: serde_yaml::Value = serde_yaml::from_str(resource).unwrap();

    let resource = yaml["Resources"][0].clone();

    let resource = serde_yaml::from_value::<ResourcePartial>(resource).unwrap();

    let res = test_srv.put("/rules").send_json(&resource).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let res = test_srv
      .delete(format!("/rules/{}", resource.name))
      .send()
      .await
      .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
  }
}
