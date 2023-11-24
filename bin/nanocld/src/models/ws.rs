use std::time::{Instant, Duration};

/// ## HEARTBEAT INTERVAL
/// How often heartbeat pings are sent
pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// ## CLIENT TIMEOUT
/// How long before lack of client response causes a timeout
pub(crate) const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// ## WsConState
///
/// This structure represent the state of a websocket connection.
///
#[derive(Debug, Clone)]
pub struct WsConState {
  /// The last heartbeat
  pub hb: Instant,
}

impl WsConState {
  /// ## New
  ///
  /// Create a new WsConState
  ///
  /// # Returns
  ///
  /// * [con_state](WsConState) - The new WsConState
  ///
  pub(crate) fn new() -> Self {
    Self { hb: Instant::now() }
  }
}
