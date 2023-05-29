use hyper::client::ResponseFuture;
use ntex::http::StatusCode;
use jsonschema::{JSONSchema, Draft};

use nanocl_stubs::resource::{Resource, ResourcePartial};

use crate::repositories;
use nanocl_utils::http_error::HttpError;
use crate::models::{Pool, ResourceKindPartial};

use super::ctrl_client::{CtrlClient, self};

/// Validate a resource from a custom config
pub async fn validate_resource(
  resource: &ResourcePartial,
  pool: &Pool,
) -> Result<(), HttpError> {
  match resource.kind.as_str() {
    "Custom" => {
      let resource_kind = ResourceKindPartial {
        name: resource.name.to_owned(),
        version: resource.version.to_owned(),
        schema: resource.config.clone(),
      };

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
      let schema: JSONSchema = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&kind.schema)
        .map_err(|err| HttpError {
          status: StatusCode::BAD_REQUEST,
          msg: format!("Invalid schema {}", err),
        })?;
      schema.validate(&resource.config).map_err(|err| {
        let mut msg = String::from("Invalid config ");
        for error in err {
          msg += &format!("{} ", error);
        }
        HttpError {
          status: StatusCode::BAD_REQUEST,
          msg,
        }
      })?;
    }
  }
  Ok(())
}

async fn get_resource_client(
  kind: &str,
  version: &str,
  pool: &Pool,
) -> Result<CtrlClient, HttpError> {
  let kind =
    repositories::resource_kind::get_version(kind, version, pool).await?;
  let url = "kind.url.as_str()";
  let name = kind.resource_kind_name.as_str();
  let ctrl_client = CtrlClient::new(name, url);

  Ok(ctrl_client)
}

/// Hook when creating a resource
async fn hook_create_resource(
  resource: &ResourcePartial,
  pool: &Pool,
) -> Result<ResourcePartial, HttpError> {
  println!("{}", resource.kind);
  validate_resource(resource, pool).await?;
  println!("validated");

  let ctrl_client =
    get_resource_client(&resource.kind, &resource.version, pool).await?;

  let config = ctrl_client
    .apply_rule(&resource.version, &resource.name, &resource.config)
    .await?;

  let mut resource = resource.clone();
  resource.config = config;

  Ok(resource)
}

/// Hook when deleting a resource
async fn hook_delete_resource(
  resource: &Resource,
  pool: &Pool,
) -> Result<(), HttpError> {
  let ctrl_client =
    get_resource_client(&resource.kind, &resource.version, pool).await?;

  ctrl_client
    .delete_rule(&resource.version, &resource.name)
    .await?;
  Ok(())
}

/// Create a resource
pub async fn create(
  resource: &ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpError> {
  hook_create_resource(resource, pool).await?;
  let res = repositories::resource::create(resource, pool).await?;
  Ok(res)
}

/// Patch a resource
pub async fn patch(
  resource: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpError> {
  hook_create_resource(&resource, pool).await?;
  let res = repositories::resource::patch(&resource, pool).await?;
  Ok(res)
}

/// Create or patch a resource
pub async fn create_or_patch(
  resource: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpError> {
  hook_create_resource(&resource, pool).await?;
  let res = repositories::resource::create_or_patch(&resource, pool).await?;
  Ok(res)
}

/// Delete a resource
pub async fn delete(resource: Resource, pool: &Pool) -> Result<(), HttpError> {
  if let Err(err) = hook_delete_resource(&resource, pool).await {
    log::warn!("{err}");
  }
  if resource.kind.as_str() == "Custom" {
    repositories::resource_kind::delete_version(&resource.name, pool).await?;
    repositories::resource_kind::delete(&resource.name, pool).await?;
  }
  repositories::resource::delete_by_key(&resource.name, pool).await?;
  repositories::resource_config::delete_by_resource_key(&resource.name, pool)
    .await?;
  Ok(())
}
