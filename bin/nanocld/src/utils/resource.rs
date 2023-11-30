use diesel::ExpressionMethods;
use ntex::http;
use serde_json::Value;
use jsonschema::{Draft, JSONSchema};

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::system::EventAction;
use nanocl_stubs::resource::{Resource, ResourcePartial};

use crate::models::{
  Pool, ResourceKindPartial, DaemonState, ResourceKindVersionDb, Repository,
  ResourceKindDb, ResourceSpecDb, ResourceDb,
};

use super::ctrl_client::CtrlClient;

/// This hook is called when a resource is created.
/// It call a custom controller at a specific url or just validate a schema.
/// If the resource is a Kind Kind, it will create a resource Kind with an associated version.
/// To call a custom controller, the resource Kind must have a Url field in his config.
/// Unless it must have a Schema field in his config that is a JSONSchema to validate the resource.
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
      if ResourceKindDb::find_by_pk(&resource.name, pool)
        .await?
        .is_err()
      {
        ResourceKindDb::create(&resource_kind, pool).await??;
      }
      ResourceKindVersionDb::create(&resource_kind, pool).await??;
    }
    _ => {
      let kind = ResourceKindVersionDb::get_version(
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

/// This hook is called when a resource is deleted.
/// It call a custom controller at a specific url.
/// If the resource is a Kind Kind, it will delete the resource Kind with an associated version.
async fn hook_delete_resource(
  resource: &Resource,
  pool: &Pool,
) -> HttpResult<()> {
  let kind = ResourceKindVersionDb::get_version(
    &resource.kind,
    &resource.spec.version,
    pool,
  )
  .await?;
  if let Some(url) = kind.url {
    let ctrl_client = CtrlClient::new(&kind.resource_kind_name, &url);
    ctrl_client
      .delete_rule(&resource.spec.version, &resource.spec.resource_key)
      .await?;
  }
  Ok(())
}

/// This function create a resource.
/// It will call the hook_create_resource function to hook the resource.
pub(crate) async fn create(
  resource: &ResourcePartial,
  state: &DaemonState,
) -> HttpResult<Resource> {
  if ResourceDb::inspect_by_pk(&resource.name, &state.pool)
    .await
    .is_ok()
  {
    return Err(HttpError {
      status: http::StatusCode::CONFLICT,
      msg: format!("Resource {} already exists", &resource.name),
    });
  }
  let resource = hook_create_resource(resource, &state.pool).await?;
  let res = ResourceDb::create_from_spec(&resource, &state.pool).await?;
  state
    .event_emitter
    .spawn_emit_to_event(&res, EventAction::Created);
  Ok(res)
}

/// This function patch a resource.
/// It will call the hook_create_resource function to hook the resource.
pub(crate) async fn patch(
  resource: &ResourcePartial,
  state: &DaemonState,
) -> HttpResult<Resource> {
  let resource = hook_create_resource(resource, &state.pool).await?;
  let res = ResourceDb::update_from_spec(&resource, &state.pool).await?;
  state
    .event_emitter
    .spawn_emit_to_event(&res, EventAction::Patched);
  Ok(res)
}

/// This function delete a resource.
/// It will call the hook_delete_resource function to hook the resource.
pub(crate) async fn delete(
  resource: &Resource,
  state: &DaemonState,
) -> HttpResult<()> {
  if let Err(err) = hook_delete_resource(resource, &state.pool).await {
    log::warn!("{err}");
  }
  if resource.kind.as_str() == "Kind" {
    let pk = resource.spec.resource_key.as_str();
    // ResourceKindVersionDb::delete_by_pk(pk, &state.pool).await??;
    ResourceKindDb::delete_by_pk(pk, &state.pool).await??;
  }
  ResourceDb::delete_by_pk(&resource.spec.resource_key, &state.pool).await??;
  ResourceSpecDb::delete_by(
    crate::schema::resource_specs::dsl::resource_key
      .eq(resource.spec.resource_key.clone()),
    &state.pool,
  )
  .await??;
  state
    .event_emitter
    .spawn_emit_to_event(resource, EventAction::Deleted);
  Ok(())
}

/// This function delete a resource by key.
/// It will call the hook_delete_resource function to hook the resource.
pub(crate) async fn delete_by_key(
  key: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let resource = ResourceDb::inspect_by_pk(key, &state.pool).await?;
  delete(&resource, state).await?;
  Ok(())
}
