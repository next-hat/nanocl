use jsonschema::{Draft, JSONSchema};

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::{
  system::NativeEventAction,
  resource_kind::ResourceKind,
  resource::{Resource, ResourcePartial},
};

use crate::models::{Pool, SystemState, SpecDb, ResourceDb};

use super::ctrl_client::CtrlClient;

/// This hook is called when a resource is created.
/// It call a custom controller at a specific url or just validate a schema.
/// If the resource is a Kind Kind, it will create a resource Kind with an associated version.
/// To call a custom controller, the resource Kind must have a Url field in his config.
/// Unless it must have a Schema field in his config that is a JSONSchema to validate the resource.
pub(crate) async fn hook_create_resource(
  resource: &ResourcePartial,
  pool: &Pool,
) -> HttpResult<ResourcePartial> {
  let mut resource = resource.clone();
  let (kind, version) = ResourceDb::parse_kind(&resource.kind, pool).await?;
  log::trace!("hook_create_resource kind: {kind} {version}");
  let kind: ResourceKind = SpecDb::get_version(&kind, &version, pool)
    .await?
    .try_into()?;
  if let Some(schema) = &kind.data.schema {
    let schema: JSONSchema = JSONSchema::options()
      .with_draft(Draft::Draft7)
      .compile(schema)
      .map_err(|err| {
        HttpError::bad_request(format!("Invalid schema {}", err))
      })?;
    schema.validate(&resource.data).map_err(|err| {
      let mut msg = String::from("Invalid config ");
      for error in err {
        msg += &format!("{} ", error);
      }
      HttpError::bad_request(msg)
    })?;
  }
  if let Some(url) = &kind.data.url {
    let ctrl_client = CtrlClient::new(&kind.name, url);
    let config = ctrl_client
      .apply_rule(&version, &resource.name, &resource.data)
      .await?;
    resource.data = config;
  }
  Ok(resource)
}

/// This hook is called when a resource is deleted.
/// It call a custom controller at a specific url.
/// If the resource is a Kind Kind, it will delete the resource Kind with an associated version.
pub(crate) async fn hook_delete_resource(
  resource: &Resource,
  pool: &Pool,
) -> HttpResult<()> {
  let (kind, version) = ResourceDb::parse_kind(&resource.kind, pool).await?;
  let kind: ResourceKind = SpecDb::get_version(&kind, &version, pool)
    .await?
    .try_into()?;
  log::debug!("hook_delete_resource kind: {kind:?}");
  if let Some(url) = &kind.data.url {
    let ctrl_client = CtrlClient::new(&kind.name, url);
    ctrl_client
      .delete_rule(&resource.spec.version, &resource.spec.resource_key)
      .await?;
  }
  Ok(())
}

/// This function patch a resource.
/// It will call the hook_create_resource function to hook the resource.
pub(crate) async fn patch(
  resource: &ResourcePartial,
  state: &SystemState,
) -> HttpResult<Resource> {
  let resource = hook_create_resource(resource, &state.pool).await?;
  let res = ResourceDb::update_from_spec(&resource, &state.pool).await?;
  super::event_emitter::emit_normal_native_action(
    &res,
    NativeEventAction::Patch,
    state,
  );
  Ok(res)
}
