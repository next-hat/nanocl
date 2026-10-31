use std::sync::Arc;

use ntex::web;

use nanocl_error::io::IoResult;

use nanocl_utils::ntex::middlewares;

use crate::{models::SystemStateRef, services};

pub fn generate(state: &SystemStateRef) -> IoResult<ntex::server::Server> {
  let state = Arc::clone(state);
  let mut server = web::server_with_config(
    async move || {
      Ok::<_, std::io::Error>(web::AppState::new(Arc::clone(&state)))
    },
    async |_| {
      web::App::<web::AppState<SystemStateRef>>::new()
        .with_config(services::app_config())
        .middleware(middlewares::SerializeError)
        .configure(services::ntex_config)
        .default_service(web::route().to(services::unhandled))
    },
  );
  std::fs::create_dir_all("/run/nanocl")?;
  server =
    server.bind_uds("/run/nanocl/proxy.sock", ntex::SharedCfg::default())?;
  #[cfg(feature = "dev")]
  {
    server = server.bind("0.0.0.0:8686", ntex::SharedCfg::default())?;
    log::debug!("server::gen: dev mode http://0.0.0.0:8686");
    log::debug!("server::gen: swagger http://0.0.0.0:8686/explorer/");
  }
  server = server.workers(num_cpus::get());
  Ok(server.run())
}
