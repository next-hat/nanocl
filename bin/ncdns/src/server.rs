use ntex::web;

use nanocl_error::io::{IoError, IoResult};
use nanocl_utils::ntex::middlewares;

use crate::models::SystemState;
use crate::services;

pub fn generate(
  host: &str,
  state: &SystemState,
) -> IoResult<ntex::server::Server> {
  let state = state.clone();
  let mut server = web::server_with_config(
    async move || Ok::<_, std::io::Error>(state.clone()),
    async |_| {
      web::App::<SystemState>::new()
        .with_config(services::app_config())
        .middleware(middlewares::SerializeError)
        .configure(services::ntex_config)
        .default_service(web::route().to(services::unhandled))
    },
  );
  match host {
    host if host.starts_with("unix://") => {
      let path = host.trim_start_matches("unix://");
      server = server.bind_uds(path, ntex::SharedCfg::default())?;
    }
    host if host.starts_with("tcp://") => {
      let host = host.trim_start_matches("tcp://");
      server = server.bind(host, ntex::SharedCfg::default())?;
    }
    _ => {
      return Err(IoError::invalid_data(
        "Server",
        "invalid host format (must be unix:// or tcp://)",
      ));
    }
  }
  #[cfg(feature = "dev")]
  {
    server = server.bind("0.0.0.0:8787", ntex::SharedCfg::default())?;
    log::debug!("server::gen: dev mode http://0.0.0.0:8787");
    log::debug!("server::gen: swagger http://0.0.0.0:8787/explorer/");
  }
  server = server.workers(num_cpus::get());
  Ok(server.run())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::dnsmasq::Dnsmasq;
  use nanocl_error::io::IoResult;
  use nanocld_client::{ConnectOpts, NanocldClient};

  #[ntex::test]
  async fn generate_unix_and_tcp() -> IoResult<()> {
    let dnsmasq = Dnsmasq::new("/tmp/ncdns");
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })?;
    let state = SystemState::new(dnsmasq, client);
    let server = generate("unix:///tmp/ncdns.sock", &state)?;
    server.stop(true).await;
    let server = generate("tcp://0.0.0.0:9987", &state)?;
    server.stop(true).await;
    Ok(())
  }

  #[ntex::test]
  async fn generate_wrong_host() -> IoResult<()> {
    let dnsmasq = Dnsmasq::new("/tmp/ncdns");
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })?;
    let state = SystemState::new(dnsmasq, client);
    let server = generate("wrong://dsadsa", &state);
    assert!(server.is_err());
    Ok(())
  }
}
