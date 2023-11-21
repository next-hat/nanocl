use nanocl_error::http::HttpResult;

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::secret::{Secret, SecretPartial, SecretUpdate};
use nanocl_stubs::system::EventAction;

use crate::repositories;
use crate::models::DaemonState;

pub(crate) async fn create(
  item: &SecretPartial,
  state: &DaemonState,
) -> HttpResult<Secret> {
  let secret = repositories::secret::create(item, &state.pool).await?;
  let secret: Secret = secret.into();
  state
    .event_emitter
    .spawn_emit(&secret, EventAction::Created);
  Ok(secret)
}

pub(crate) async fn delete_by_key(
  key: &str,
  state: &DaemonState,
) -> HttpResult<GenericDelete> {
  let secret = repositories::secret::find_by_key(key, &state.pool).await?;
  let res = repositories::secret::delete_by_key(key, &state.pool).await?;
  let secret: Secret = secret.into();
  state
    .event_emitter
    .spawn_emit(&secret, EventAction::Deleted);
  Ok(res)
}

pub(crate) async fn patch_by_key(
  key: &str,
  item: &SecretUpdate,
  state: &DaemonState,
) -> HttpResult<Secret> {
  let secret =
    repositories::secret::update_by_key(key, item, &state.pool).await?;
  let secret: Secret = secret.into();
  state
    .event_emitter
    .spawn_emit(&secret, EventAction::Patched);
  Ok(secret)
}
