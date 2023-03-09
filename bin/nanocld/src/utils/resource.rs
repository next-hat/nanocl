use ntex::http::StatusCode;
use jsonschema::{JSONSchema, Draft};

use nanocl_stubs::resource::{Resource, ResourcePartial};

use crate::repositories;
use crate::error::HttpResponseError;
use crate::models::{Pool, ResourceKindPartial};

pub async fn validate_resource(
  resource: &ResourcePartial,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
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
        repositories::resource_kind::create(resource_kind.clone(), pool)
          .await?;
      }
      repositories::resource_kind::create_version(resource_kind, pool).await?;
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
        .map_err(|err| HttpResponseError {
          status: StatusCode::BAD_REQUEST,
          msg: format!("Invalid schema: {}", err),
        })?;
      schema.validate(&resource.config).map_err(|err| {
        let mut msg = String::from("Invalid config: ");
        for error in err {
          msg += &format!("{} ", error);
        }
        HttpResponseError {
          status: StatusCode::BAD_REQUEST,
          msg,
        }
      })?;
    }
  }
  Ok(())
}

pub async fn create(
  resource: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpResponseError> {
  validate_resource(&resource, pool).await?;
  repositories::resource::create(resource.clone(), pool).await
}

pub async fn patch(
  resource: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpResponseError> {
  validate_resource(&resource, pool).await?;
  repositories::resource::patch(&resource, pool).await
}

pub async fn create_or_patch(
  resource: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpResponseError> {
  validate_resource(&resource, pool).await?;
  repositories::resource::create_or_patch(&resource, pool).await
}
