use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::time::Instant;

/// One bounded QMP session. Only the container utilities expose commands.
pub(crate) struct QmpClient<S = UnixStream> {
  pub(crate) stream: BufReader<S>,
  pub(crate) deadline: Instant,
  pub(crate) next_id: u64,
  /// An interrupted command leaves its outcome uncertain; reconnect to retry.
  pub(crate) failed: bool,
}
