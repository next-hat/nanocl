use std::sync::Arc;

use nanocl_error::io::IoResult;

use nanocld_client::NanocldClient;

use crate::{
  cli::Cli,
  models::{EventEmitter, Store, SystemState, SystemStateRef},
};

use super::{event, logger};

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
  let event_emitter = EventEmitter::new(&client, cli.state_dir.clone());
  let state = Arc::new(SystemState {
    client,
    event_emitter,
    store: Store::new(&cli.state_dir),
    haproxy_dir: cli.haproxy_dir.clone(),
  });
  // Initialize HAProxy configuration on startup
  if let Err(err) = crate::utils::haproxy::ensure_conf(&state).await {
    log::warn!("init: failed to initialize HAProxy configuration: {err}");
  }
  // Recover runtime state from state files after restart
  if let Err(err) = crate::utils::haproxy::recover_from_state(&state).await {
    log::warn!("init: failed to recover HAProxy runtime state: {err}");
  }
  // Watch for nanocld events and sync proxy rules
  event::spawn(&state);
  // Receive HAProxy logs over unix datagram sockets and POST metrics directly to nanocld
  logger::spawn(&state);
  Ok(state)
}
