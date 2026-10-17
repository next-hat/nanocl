/// In-memory position and incomplete record for one Nginx access log.
#[derive(Default)]
pub(crate) struct MetricCursor {
  pub identity: Option<(u64, u64)>,
  pub offset: u64,
  pub pending: Vec<u8>,
  pub discard: bool,
  pub checkpoint: Vec<u8>,
}
