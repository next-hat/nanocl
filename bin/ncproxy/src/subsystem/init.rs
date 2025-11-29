use std::sync::Arc;

use nanocl_error::io::IoResult;

use nanocld_client::NanocldClient;

use crate::{
  cli::Cli,
  models::{EventEmitter, Store, SystemState, SystemStateRef},
};

use super::{event, logger, metric};

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
  event::spawn(&state);
  // Receive HAProxy logs over unix datagram socket and persist to files
  logger::spawn(&state);
  metric::spawn(&state);
  Ok(state)
}
