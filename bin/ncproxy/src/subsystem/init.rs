use std::sync::Arc;

use nanocl_error::io::IoResult;

use nanocld_client::NanocldClient;

use crate::{
  cli::Cli,
  models::{EventEmitter, Store, SystemState, SystemStateRef},
  utils::nginx,
};

use super::{event, metric};

pub async fn init(cli: &Cli) -> IoResult<SystemStateRef> {
  #[cfg(any(feature = "dev", feature = "test"))]
  let client = {
    use nanocld_client::ConnectOpts;
    NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })?
  };
  #[cfg(not(any(feature = "dev", feature = "test")))]
  let client = NanocldClient::connect_with_unix_default();
  let config_lock = Arc::new(futures::lock::Mutex::new(()));
  let event_emitter: EventEmitter =
    EventEmitter::new(&cli.state_dir, Arc::clone(&config_lock));
  let state = Arc::new(SystemState {
    client,
    event_emitter,
    store: Store::new(&cli.state_dir),
    nginx_dir: cli.nginx_dir.clone(),
    config_lock,
  });
  // Do not block startup on nanocld availability; the event loop handles retries.
  if let Err(err) = crate::utils::nginx::ensure_conf(&state).await {
    log::warn!("init: unable to ensure nginx conf at startup: {err}");
  }
  nginx::spawn(&state);
  event::spawn(&state);
  metric::spawn(&state);
  Ok(state)
}
