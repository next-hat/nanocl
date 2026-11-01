use std::sync::Arc;

use nanocl_error::io::IoResult;

use nanocld_client::NanocldClient;

use crate::{
  cli::Cli,
  models::{EventEmitter, Store, SystemState, SystemStateRef},
  utils::nginx,
};

use super::event;

pub async fn init(cli: &Cli) -> IoResult<SystemStateRef> {
  let controller_locks = nginx::controller_locks(&cli.state_dir)?;
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
    _controller_locks: Some(controller_locks),
  });
  // Admission and the first configuration/start happen before any background
  // task can mutate files or race an uncertain predecessor.
  nginx::initialize(&state).await?;
  nginx::spawn(&state);
  event::spawn(&state);
  Ok(state)
}
