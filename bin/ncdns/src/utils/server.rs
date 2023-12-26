use std::sync::Arc;

use ntex::web;

use nanocl_error::io::{IoResult, IoError};
use nanocl_utils::ntex::middlewares;

use crate::{services, models::SystemStateRef};

pub fn gen(
  host: &str,
  state: &SystemStateRef,
) -> IoResult<ntex::server::Server> {
  let state = Arc::clone(state);
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      .state(Arc::clone(&state))
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
    log::debug!("server::gen: dev mode http://0.0.0.0:8787");
    log::debug!("server::gen: swagger http://0.0.0.0:8787/explorer/");
  }
  Ok(server.run())
}
