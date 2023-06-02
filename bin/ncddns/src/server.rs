use nanocl_utils::http_client_error::HttpClientError;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::resource::ResourcePartial;
use ntex::web;

use nanocl_utils::ntex::middlewares;
use nanocl_utils::io_error::{IoResult, IoError};

use crate::services;
use crate::dnsmasq::Dnsmasq;

async fn create_dns_rule_kind() -> Result<(), HttpClientError> {
  let client = NanocldClient::connect_with_unix_default();
  let dns_rule_kind = ResourcePartial {
    kind: "Kind".to_string(),
    name: "DnsRule".to_string(),
    config: serde_json::json!({
        "Url": "unix:///run/nanocl/dns.sock"
    }),
    version: "v0.1".to_string(),
  };

  if let Err(err) = client.create_resource(&dns_rule_kind).await {
    match err {
      HttpClientError::HttpError(err) if err.status == 409 => {
        log::info!("DnsRule already exists. Skipping.")
      }
      _ => return Err(err),
    }
  }

  Ok(())
}

async fn ensure_basic_resources() {
  loop {
    match create_dns_rule_kind().await {
      Ok(_) => break,
      Err(_) => {
        log::warn!(
          "Failed to ensure basic resource kinds exists, retrying in 2 seconds"
        );
        ntex::time::sleep(std::time::Duration::from_secs(2)).await;
      }
    }
  }

  log::info!("DnsRule exists");
}

pub fn generate(
  host: &str,
  dnsmasq: &Dnsmasq,
) -> IoResult<ntex::server::Server> {
  let dnsmasq = dnsmasq.clone();
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      .state(dnsmasq.clone())
      .wrap(middlewares::SerializeError)
      .configure(services::ntex_config)
      .default_service(web::route().to(services::unhandled))
  });

  match host {
    host if host.starts_with("unix://") => {
      let path = host.trim_start_matches("unix://");
      server = server.bind_uds(path)?;
    }
    host if host.starts_with("tcp://") => {
      let host = host.trim_start_matches("tcp://");
      server = server.bind(host)?;
    }
    _ => {
      return Err(IoError::invalid_data(
        "Server",
        "invalid host format (must be unix:// or tcp://)",
      ))
    }
  }

  ntex::rt::spawn(async move {
    ensure_basic_resources().await;
  });

  #[cfg(feature = "dev")]
  {
    server = server.bind("0.0.0.0:8787")?;
    log::debug!("Running in dev mode, binding to: http://0.0.0.0:8787");
    log::debug!("OpenAPI explorer available at: http://0.0.0.0:8787/explorer/");
  }

  Ok(server.run())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::dnsmasq::Dnsmasq;
  use nanocl_utils::io_error::IoResult;

  #[ntex::test]
  async fn generate_unix_and_tcp() -> IoResult<()> {
    let dnsmasq = Dnsmasq::new("/tmp/ncddns");
    let server = generate("unix:///tmp/ncddns.sock", &dnsmasq)?;
    server.stop(true).await;
    let server = generate("tcp://0.0.0.0:9987", &dnsmasq)?;
    server.stop(true).await;
    Ok(())
  }

  #[test]
  fn generate_wrong_host() -> IoResult<()> {
    let dnsmasq = Dnsmasq::new("/tmp/ncddns");
    let server = generate("wrong://dsadsa", &dnsmasq);
    assert!(server.is_err());
    Ok(())
  }
}
