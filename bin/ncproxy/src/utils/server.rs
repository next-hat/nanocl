use std::sync::Arc;

use ntex::web;

use nanocl_error::io::IoResult;

use nanocl_utils::ntex::middlewares;

use crate::{services, models::SystemStateRef};

pub fn gen(state: &SystemStateRef) -> IoResult<ntex::server::Server> {
  let state = Arc::clone(state);
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      .state(Arc::clone(&state))
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
  let num = num_cpus::get();
  let workers = if num < 2 { 1 } else { num / 2 };
  server = server.workers(workers);
  Ok(server.run())
}
