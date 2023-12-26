use std::sync::Arc;

use nanocl_error::io::IoResult;

use nanocld_client::NanocldClient;

use crate::{
  utils,
  cli::Cli,
  models::{Store, SystemState, SystemStateRef, EventEmitter},
};

use super::{event, metric};

pub async fn init(cli: &Cli) -> IoResult<SystemStateRef> {
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    client = NanocldClient::connect_to("http://nanocl.internal:8585", None);
  }
  let state = Arc::new(SystemState {
    client,
    event_emitter: EventEmitter::new(),
    store: Store::new(&cli.state_dir),
    nginx_dir: cli.nginx_dir.clone(),
  });
  event::spawn(&state);
  metric::spawn(&state);
  utils::nginx::ensure_conf(&state).await?;
  #[cfg(not(feature = "test"))]
  {
    utils::nginx::spawn(&state).await?;
  }
  Ok(state)
}
