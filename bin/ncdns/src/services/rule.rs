use ntex::web;

use nanocl_error::http::HttpError;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::dns::ResourceDnsRule;

use crate::{utils, dnsmasq};

/// Create/Update a new DnsRule
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Rules",
  path = "/rules/{name}",
  request_body = ResourceDnsRule,
  params(
    ("name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "The created rule", body = ResourceDnsRule),
  ),
))]
#[web::put("/rules/{name}")]
pub(crate) async fn apply_rule(
  // To follow the ressource service convention, we have to use a tuple
  client: web::types::State<NanocldClient>,
  dnsmasq: web::types::State<dnsmasq::Dnsmasq>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ResourceDnsRule>,
) -> Result<web::HttpResponse, HttpError> {
  utils::update_entries(&path.1, &payload, &dnsmasq, &client).await?;
  utils::reload_service(&client).await?;
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
pub(crate) async fn remove_rule(
  client: web::types::State<NanocldClient>,
  dnsmasq: web::types::State<dnsmasq::Dnsmasq>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpError> {
  let rule = client.inspect_resource(&path.1).await?;
  let dns_rule = serde_json::from_value::<ResourceDnsRule>(rule.spec.data)
    .map_err(|err| {
      HttpError::bad_request(format!("Unable to serialize the DnsRule: {err}"))
    })?;
  utils::remove_entries(&dns_rule, &dnsmasq, &client).await?;
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

  use crate::utils::tests::*;

  #[ntex::test]
  async fn apply_empty_rule() {
    let client = gen_default_test_client();
    let res = client
      .send_put("/rules/test", None::<String>, None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "apply empty rule"
    );
  }

  #[ntex::test]
  async fn remove_unexisting_rule() {
    let client = gen_default_test_client();
    let res = client.send_delete("/rules/test", None::<String>).await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "remove unexisting rule"
    );
  }
}
