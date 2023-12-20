use nanocl_error::http::HttpResult;

use nanocl_stubs::{
  system::EventAction,
  secret::{Secret, SecretPartial, SecretUpdate},
};

use crate::{
  repositories::generic::*,
  models::{DaemonState, SecretDb},
};

pub(crate) async fn create(
  item: &SecretPartial,
  state: &DaemonState,
) -> HttpResult<Secret> {
  let secret = SecretDb::create_from(item, &state.pool).await??;
  let secret: Secret = secret.try_into()?;
  state
    .event_emitter
    .spawn_emit_to_event(&secret, EventAction::Created);
  Ok(secret)
}

pub(crate) async fn delete_by_pk(
  key: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let secret = SecretDb::read_by_pk(key, &state.pool).await??;
  SecretDb::del_by_pk(key, &state.pool).await??;
  state
    .event_emitter
    .spawn_emit_to_event(&secret, EventAction::Deleted);
  Ok(())
}

/// Patch a secret by it's primary key
pub(crate) async fn patch_by_pk(
  key: &str,
  item: &SecretUpdate,
  state: &DaemonState,
) -> HttpResult<Secret> {
  let secret = SecretDb::update_pk(key, item, &state.pool).await??;
  let secret: Secret = secret.try_into()?;
  state
    .event_emitter
    .spawn_emit_to_event(&secret, EventAction::Patched);
  Ok(secret)
}
