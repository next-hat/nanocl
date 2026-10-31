use ntex::web;

use nanocl_error::http::HttpError;

use nanocld_client::stubs::dns::ResourceDnsRule;

use crate::{models::SystemState, utils};

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
pub(crate) async fn apply_rule(
  // To follow the resource service convention, we have to use a tuple
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ResourceDnsRule>,
) -> Result<web::HttpResponse, HttpError> {
  log::debug!("rule::apply: resolving {}", path.1);
  let scope = utils::resolve_rule(&payload, &state.client).await?;
  log::debug!("rule::apply: applying {}", path.1);
  state.dnsmasq.apply_rule(&path.1, scope).await?;
  log::debug!("rule::apply: applied {}", path.1);
  Ok(web::HttpResponse::Ok().json(&payload.into_inner()))
}

/// Delete a DnsRule
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
pub(crate) async fn remove_rule(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpError> {
  // Nanocld invokes the controller before deleting the persisted resource.
  state.client.inspect_resource(&path.1).await?;
  log::debug!("rule::remove: removing {}", path.1);
  state.dnsmasq.remove_rule(&path.1).await?;
  log::debug!("rule::remove: removed {}", path.1);
  Ok(web::HttpResponse::Ok().build())
}

pub fn ntex_config(
  config: &mut web::ServiceConfig<crate::models::SystemState>,
) {
  config.service(
    web::resource("/rules/{name}")
      .name("apply_rule")
      .guard(web::guard::Put())
      .to_with_state(apply_rule),
  );
  config.service(
    web::resource("/rules/{name}")
      .name("remove_rule")
      .guard(web::guard::Delete())
      .to_with_state(remove_rule),
  );
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::dns::ResourceDnsRule;
  use ntex::http;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() {
    let data = std::fs::read_to_string("tests/resource_dns.yml").unwrap();
    let payload = serde_yaml::from_str::<ResourceDnsRule>(&data).unwrap();
    log::debug!("apply_rule: payload = {payload:#?}");
    let client = gen_default_test_client().await;
    let res = client
      .send_put("/rules/test", Some(payload), None::<String>)
      .await;
    let body = res.json::<serde_json::Value>().await;
    log::debug!("apply_rule: body = {body:#?}");
    test_status_code!(res.status(), http::StatusCode::OK, "basic");
  }

  #[ntex::test]
  async fn apply_empty_rule() {
    let client = gen_default_test_client().await;
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
    let client = gen_default_test_client().await;
    let res = client.send_delete("/rules/test", None::<String>).await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "remove unexisting rule"
    );
  }
}
