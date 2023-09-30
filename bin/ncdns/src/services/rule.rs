use ntex::web;

use nanocl_utils::http_error::HttpError;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::dns::ResourceDnsRule;

use crate::utils::{self, remove_entries};
use crate::dnsmasq::Dnsmasq;

/// Create/Update a new DnsRule
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Rules",
  path = "/rules/{Name}",
  request_body = ResourceDnsRule,
  params(
    ("Name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "The created rule", body = ResourceDnsRule),
  ),
))]
#[web::put("/rules/{name}")]
pub(crate) async fn apply_rule(
  // To follow the ressource service convention, we have to use a tuple
  _path: web::types::Path<(String, String)>,
  dnsmasq: web::types::State<Dnsmasq>,
  web::types::Json(payload): web::types::Json<ResourceDnsRule>,
) -> Result<web::HttpResponse, HttpError> {
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
  }
  utils::write_entries(&payload, &dnsmasq, &client).await?;
  utils::reload_service(&client).await?;
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
pub(crate) async fn remove_rule(
  path: web::types::Path<(String, String)>,
  dnsmasq: web::types::State<Dnsmasq>,
) -> Result<web::HttpResponse, HttpError> {
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
  }
  let rule = client.inspect_resource(&path.1).await?;
  let dns_rule = serde_json::from_value::<ResourceDnsRule>(rule.config)
    .map_err(|err| {
      HttpError::bad_request(format!(
        "Unable to serialize the DnsRule: {}",
        err
      ))
    })?;
  remove_entries(&dns_rule, &dnsmasq, &client).await?;
  utils::reload_service(&client).await?;
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

    let resource: &str = include_str!("../../tests/resource_dns.yml");

    let yaml: serde_yaml::Value = serde_yaml::from_str(resource).unwrap();

    let resource = yaml["Resources"][0].clone();
    let name = resource["Name"].as_str().unwrap();

    let mut res = test_srv
      .put(format!("/v0.2/rules/{name}"))
      .send_json(&resource["Config"])
      .await
      .unwrap();

    println!("{:#?}", res);
    let json = res.json::<serde_json::Value>().await.unwrap();
    println!("{:#?}", json);
    assert_eq!(res.status(), http::StatusCode::OK);
    let res = test_srv
      .delete(format!("/v0.2/rules/{name}"))
      .send()
      .await
      .unwrap();

    assert_eq!(res.status(), http::StatusCode::OK);
  }
}
