use ntex::http;
use serde_json::Value;
use jsonschema::{Draft, JSONSchema};

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::system::EventAction;
use nanocl_stubs::resource::{Resource, ResourcePartial};

use crate::repositories;
use crate::models::{Pool, ResourceKindPartial, DaemonState};

use super::ctrl_client::CtrlClient;

/// ## Hook create resource
///
/// This hook is called when a resource is created.
/// It call a custom controller at a specific url or just validate a schema.
/// If the resource is a Kind Kind, it will create a resource Kind with an associated version.
/// To call a custom controller, the resource Kind must have a Url field in his config.
/// Unless it must have a Schema field in his config that is a JSONSchema to validate the resource.
///
/// ## Arguments
///
/// * [resource](ResourcePartial) - The resource to create
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [ResourcePartial](ResourcePartial)
///
async fn hook_create_resource(
  resource: &ResourcePartial,
  pool: &Pool,
) -> HttpResult<ResourcePartial> {
  let mut resource = resource.clone();
  match resource.kind.as_str() {
    "Kind" => {
      // Todo: validate the resource with a structure
      let resource_kind = ResourceKindPartial {
        name: resource.name.to_owned(),
        version: resource.version.to_owned(),
        schema: resource.data.get("Schema").cloned(),
        url: resource.data.get("Url").map(|item| match item {
          Value::String(value) => value.clone(),
          // Wtf ? so if it's not a string, we just convert it to a string ?
          // Meaning that if it's a number an array or whatever, it will be converted to a string ?
          value => value.to_string(),
        }),
      };
      if resource_kind.schema.is_none() && resource_kind.url.is_none() {
        return Err(HttpError {
          msg: "Neither schema nor url provided".to_owned(),
          status: http::StatusCode::BAD_REQUEST,
        });
      }
      if repositories::resource_kind::find_by_name(&resource.name, pool)
        .await
        .is_err()
      {
        repositories::resource_kind::create(&resource_kind, pool).await?;
      }
      repositories::resource_kind::create_version(&resource_kind, pool).await?;
    }
    _ => {
      let kind = repositories::resource_kind::get_version(
        &resource.kind,
        &resource.version,
        pool,
      )
      .await?;
      if let Some(schema) = kind.schema {
        let schema: JSONSchema = JSONSchema::options()
          .with_draft(Draft::Draft7)
          .compile(&schema)
          .map_err(|err| HttpError {
            status: http::StatusCode::BAD_REQUEST,
            msg: format!("Invalid schema {}", err),
          })?;
        schema.validate(&resource.data).map_err(|err| {
          let mut msg = String::from("Invalid config ");
          for error in err {
            msg += &format!("{} ", error);
          }
          HttpError {
            status: http::StatusCode::BAD_REQUEST,
            msg,
          }
        })?;
      }
      if let Some(url) = kind.url {
        let ctrl_client = CtrlClient::new(&kind.resource_kind_name, &url);
        let config = ctrl_client
          .apply_rule(&resource.version, &resource.name, &resource.data)
          .await?;
        resource.data = config;
      }
    }
  }
  Ok(resource)
}

/// ## Hook delete resource
///
/// This hook is called when a resource is deleted.
/// It call a custom controller at a specific url.
/// If the resource is a Kind Kind, it will delete the resource Kind with an associated version.
///
/// ## Arguments
///
/// * [resource](Resource) - The resource to delete
/// * [pool](Pool) - The database pool
///
async fn hook_delete_resource(
  resource: &Resource,
  pool: &Pool,
) -> HttpResult<()> {
  let kind = repositories::resource_kind::get_version(
    &resource.kind,
    &resource.version,
    pool,
  )
  .await?;
  if let Some(url) = kind.url {
    let ctrl_client = CtrlClient::new(&kind.resource_kind_name, &url);
    ctrl_client
      .delete_rule(&resource.version, &resource.name)
      .await?;
  }
  Ok(())
}

/// ## Create
///
/// This function create a resource.
/// It will call the hook_create_resource function to hook the resource.
///
/// ## Arguments
///
/// * [resource](ResourcePartial) - The resource to create
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Resource](Resource)
///
pub(crate) async fn create(
  resource: &ResourcePartial,
  state: &DaemonState,
) -> HttpResult<Resource> {
  if repositories::resource::inspect_by_key(&resource.name, &state.pool)
    .await
    .is_ok()
  {
    return Err(HttpError {
      status: http::StatusCode::CONFLICT,
      msg: format!("Resource {} already exists", &resource.name),
    });
  }
  let resource = hook_create_resource(resource, &state.pool).await?;
  let res = repositories::resource::create(&resource, &state.pool).await?;
  state
    .event_emitter
    .spawn_emit_to_event(&res, EventAction::Created);
  Ok(res)
}

/// ## Patch
///
/// This function patch a resource.
/// It will call the hook_create_resource function to hook the resource.
///
/// ## Arguments
///
/// * [resource](ResourcePartial) - The resource to patch
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Resource](Resource)
///
pub(crate) async fn patch(
  resource: &ResourcePartial,
  state: &DaemonState,
) -> HttpResult<Resource> {
  let resource = hook_create_resource(resource, &state.pool).await?;
  let res = repositories::resource::put(&resource, &state.pool).await?;
  state
    .event_emitter
    .spawn_emit_to_event(&res, EventAction::Patched);
  Ok(res)
}

/// ## Delete
///
/// This function delete a resource.
/// It will call the hook_delete_resource function to hook the resource.
///
/// ## Arguments
///
/// * [resource](Resource) - The resource to delete
/// * [state](DaemonState) - The daemon state
///
pub(crate) async fn delete(
  resource: &Resource,
  state: &DaemonState,
) -> HttpResult<()> {
  if let Err(err) = hook_delete_resource(resource, &state.pool).await {
    log::warn!("{err}");
  }
  if resource.kind.as_str() == "Kind" {
    repositories::resource_kind::delete_version(&resource.name, &state.pool)
      .await?;
    repositories::resource_kind::delete(&resource.name, &state.pool).await?;
  }
  repositories::resource::delete_by_key(&resource.name, &state.pool).await?;
  repositories::resource_spec::delete_by_resource_key(
    &resource.name,
    &state.pool,
  )
  .await?;
  state
    .event_emitter
    .spawn_emit_to_event(resource, EventAction::Deleted);
  Ok(())
}

/// ## Delete by key
///
/// This function delete a resource by key.
/// It will call the hook_delete_resource function to hook the resource.
///
/// ## Arguments
///
/// * [key](str) - The resource key
/// * [state](DaemonState) - The daemon state
///
pub(crate) async fn delete_by_key(
  key: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let resource =
    repositories::resource::inspect_by_key(key, &state.pool).await?;
  delete(&resource, state).await?;
  Ok(())
}
