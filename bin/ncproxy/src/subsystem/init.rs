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
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    use nanocld_client::ConnectOpts;
    client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })?;
  }
  let event_emitter: EventEmitter = EventEmitter::new(&cli.state_dir);
  let state = Arc::new(SystemState {
    client,
    event_emitter,
    store: Store::new(&cli.state_dir),
    nginx_dir: cli.nginx_dir.clone(),
  });
  // Do not block startup on nanocld availability; the event loop handles retries.
  if let Err(err) = crate::utils::nginx::ensure_conf(&state).await {
    log::warn!("init: unable to ensure nginx conf at startup: {err}");
  }
  nginx::spawn(&state.store.dir);
  event::spawn(&state);
  metric::spawn(&state);
  Ok(state)
}
