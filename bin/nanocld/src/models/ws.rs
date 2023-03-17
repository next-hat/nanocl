use std::time::{Instant, Duration};

/// How often heartbeat pings are sent
pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
pub(crate) const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct WsConState {
  pub(crate) hb: Instant,
}

impl WsConState {
  pub(crate) fn new() -> Self {
    Self { hb: Instant::now() }
  }
}
