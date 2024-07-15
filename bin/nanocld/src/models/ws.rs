use std::time::{Duration, Instant};

/// How often heartbeat pings are sent
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// This structure represent the state of a websocket connection.
#[derive(Debug, Clone)]
pub struct WsConState {
  /// The last heartbeat
  pub hb: Instant,
}

impl WsConState {
  /// Create a new WsConState
  pub fn new() -> Self {
    Self { hb: Instant::now() }
  }
}
