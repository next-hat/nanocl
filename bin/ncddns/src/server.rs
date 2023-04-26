use ntex::web;

use nanocl_utils::ntex::middlewares;
use nanocl_utils::io_error::IoResult;

use crate::services;
use crate::dnsmasq::Dnsmasq;

pub fn generate(dnsmasq: &Dnsmasq) -> IoResult<ntex::server::Server> {
  let dnsmasq = dnsmasq.clone();
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      .state(dnsmasq.clone())
      .wrap(middlewares::SerializeError)
      .configure(services::ntex_config)
      .default_service(web::route().to(services::unhandled))
  });

  server = server.bind_uds("/run/nanocl/dns.sock")?;

  #[cfg(feature = "dev")]
  {
    server = server.bind("0.0.0.0:8787")?;
    log::debug!("Running in dev mode, binding to: http://0.0.0.0:8787");
    log::debug!("OpenAPI explorer available at: http://0.0.0.0:8787/explorer/");
  }

  Ok(server.run())
}
