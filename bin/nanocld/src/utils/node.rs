use nanocl_error::io::IoResult;

use crate::models::{DaemonState, NodeDb};

pub async fn register(state: &DaemonState) -> IoResult<()> {
  let node = NodeDb {
    name: state.config.hostname.clone(),
    ip_address: state.config.gateway.clone(),
    created_at: chrono::Utc::now().naive_utc(),
  };
  NodeDb::create_if_not_exists(&node, &state.pool).await?;
  Ok(())
}
