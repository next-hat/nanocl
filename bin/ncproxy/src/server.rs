use ntex::web;

use nanocld_client::NanocldClient;
use nanocl_utils::ntex::middlewares;

use crate::services;
use crate::nginx::Nginx;

pub fn gen(
  nginx: &Nginx,
  client: &NanocldClient,
) -> std::io::Result<ntex::server::Server> {
  let nginx = nginx.clone();
  let client = client.clone();
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      .state(nginx.clone())
      .state(client.clone())
      .wrap(middlewares::SerializeError)
      .configure(services::ntex_config)
      .default_service(web::route().to(services::unhandled))
  });
  server = server.bind_uds("/run/nanocl/proxy.sock")?;
  #[cfg(feature = "dev")]
  {
    server = server.bind("0.0.0.0:8686")?;
    log::debug!("server::gen: dev mode http://0.0.0.0:8686");
    log::debug!("server::gen: swagger http://0.0.0.0:8686/explorer/");
  }
  Ok(server.run())
}
