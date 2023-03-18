use crate::models::DaemonState;
use crate::error::HttpResponseError;

pub async fn list(state: &DaemonState) -> Result<(), HttpResponseError> {
  Ok(())
}
