use nanocld_client::NanocldClient;
use ntex::web;

use nanocld_client::stubs::cargo::CargoInspect;

use crate::utils;
use crate::dnsmasq::Dnsmasq;
use crate::error::HttpError;

#[web::post("/dns/entry")]
async fn dns_entry(
  dnsmasq: web::types::State<Dnsmasq>,
  web::types::Json(payload): web::types::Json<CargoInspect>,
) -> Result<web::HttpResponse, HttpError> {
  let client = NanocldClient::connect_with_unix_default();
  let domains = utils::gen_cargo_domains(&payload)?;
  dnsmasq.generate_domains_file(&payload.key, &domains)?;
  utils::restart_dns_service(&client).await?;
  Ok(web::HttpResponse::Ok().finish())
}

#[web::delete("/dns/entry/{key}")]
async fn dns_entry_delete() -> Result<web::HttpResponse, HttpError> {
  Ok(web::HttpResponse::Ok().finish())
}

pub fn configure(config: &mut web::ServiceConfig) {
  config.service(dns_entry);
  config.service(dns_entry_delete);
}
