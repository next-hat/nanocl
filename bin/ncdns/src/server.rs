use ntex::web;

use nanocl_utils::ntex::middlewares;
use nanocl_error::io::{IoResult, IoError};

use crate::services;
use crate::dnsmasq::Dnsmasq;

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
  use nanocl_error::io::IoResult;

  #[ntex::test]
  async fn generate_unix_and_tcp() -> IoResult<()> {
    let dnsmasq = Dnsmasq::new("/tmp/ncdns");
    let server = generate("unix:///tmp/ncdns.sock", &dnsmasq)?;
    server.stop(true).await;
    let server = generate("tcp://0.0.0.0:9987", &dnsmasq)?;
    server.stop(true).await;
    Ok(())
  }

  #[test]
  fn generate_wrong_host() -> IoResult<()> {
    let dnsmasq = Dnsmasq::new("/tmp/ncdns");
    let server = generate("wrong://dsadsa", &dnsmasq);
    assert!(server.is_err());
    Ok(())
  }
}
