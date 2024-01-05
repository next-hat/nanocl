use nanocl_error::http::HttpResult;

use nanocl_stubs::{
  system::NativeEventAction,
  secret::{Secret, SecretPartial, SecretUpdate},
};

use crate::{
  repositories::generic::*,
  models::{SystemState, SecretDb},
};

pub(crate) async fn create(
  item: &SecretPartial,
  state: &SystemState,
) -> HttpResult<Secret> {
  let secret = SecretDb::create_from(item, &state.pool).await?;
  let secret: Secret = secret.try_into()?;
  super::event_emitter::emit_normal_native_action(
    &secret,
    NativeEventAction::Create,
    state,
  );
  Ok(secret)
}

pub(crate) async fn delete_by_pk(
  key: &str,
  state: &SystemState,
) -> HttpResult<()> {
  let secret = SecretDb::transform_read_by_pk(key, &state.pool).await?;
  SecretDb::del_by_pk(key, &state.pool).await?;
  super::event_emitter::emit_normal_native_action(
    &secret,
    NativeEventAction::Delete,
    state,
  );
  Ok(())
}

/// Patch a secret by it's primary key
pub(crate) async fn patch_by_pk(
  key: &str,
  item: &SecretUpdate,
  state: &SystemState,
) -> HttpResult<Secret> {
  let secret = SecretDb::update_pk(key, item, &state.pool).await?;
  let secret: Secret = secret.try_into()?;
  super::event_emitter::emit_normal_native_action(
    &secret,
    NativeEventAction::Patch,
    state,
  );
  Ok(secret)
}
