use ntex::web;

use nanocl_error::http::HttpError;

use nanocld_client::stubs::dns::ResourceDnsRule;

use crate::{utils, models::SystemStateRef};

/// Create/Update a ndns.io/rule
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
  state: web::types::State<SystemStateRef>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ResourceDnsRule>,
) -> Result<web::HttpResponse, HttpError> {
  utils::rule::update_entries(&path.1, &payload, &state).await?;
  utils::rule::reload_service(&state.client).await?;
  Ok(web::HttpResponse::Ok().json(&payload.into_inner()))
}

/// Delete a ndns.io/rule
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
  state: web::types::State<SystemStateRef>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpError> {
  let rule = state.client.inspect_resource(&path.1).await?;
  let dns_rule = serde_json::from_value::<ResourceDnsRule>(rule.spec.data)
    .map_err(|err| {
      HttpError::bad_request(format!("Unable to serialize the DnsRule: {err}"))
    })?;
  utils::rule::remove_entries(&dns_rule, &state).await?;
  utils::rule::reload_service(&state.client).await?;
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
