use ntex::http::StatusCode;
use jsonschema::{JSONSchema, Draft};

use nanocl_stubs::resource::{Resource, ResourcePartial};
use nanocl_stubs::proxy::{ProxyRule, ResourceProxyRule};

use crate::repositories;
use crate::error::HttpError;
use crate::models::{Pool, ResourceKindPartial};

use super::proxy::ProxyClient;

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

async fn hook_apply_proxy_rule(
  resource: &ResourcePartial,
  pool: &Pool,
) -> Result<ResourcePartial, HttpError> {
  if resource.kind != "ProxyRule" {
    validate_resource(resource, pool).await?;
    return Ok(resource.to_owned());
  }
  let proxy = ProxyClient::unix_default();
  let hooked_resource =
    proxy.apply_rule(resource).await.map_err(|err| HttpError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("{}", err),
    })?;
  Ok(hooked_resource)
}

async fn hook_remove_proxy_rule(resource: &Resource) -> Result<(), HttpError> {
  if resource.kind != "ProxyRule" {
    return Ok(());
  }

  let proxy_rule =
    serde_json::from_value::<ResourceProxyRule>(resource.config.clone())
      .map_err(|err| HttpError {
        status: StatusCode::BAD_REQUEST,
        msg: format!("{}", err),
      })?;

  let kind = match proxy_rule.rule {
    ProxyRule::Http(_) => "site",
    ProxyRule::Stream(_) => "stream",
  };

  let proxy = ProxyClient::unix_default();
  proxy
    .delete_rule(&resource.name, kind)
    .await
    .map_err(|err| HttpError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("{}", err),
    })?;
  Ok(())
}

pub async fn create(
  resource: &ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpError> {
  hook_apply_proxy_rule(resource, pool).await?;
  repositories::resource::create(resource, pool).await
}

pub async fn patch(
  resource: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpError> {
  hook_apply_proxy_rule(&resource, pool).await?;
  repositories::resource::patch(&resource, pool).await
}

pub async fn create_or_patch(
  resource: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpError> {
  hook_apply_proxy_rule(&resource, pool).await?;
  repositories::resource::create_or_patch(&resource, pool).await
}

pub async fn delete(resource: Resource, pool: &Pool) -> Result<(), HttpError> {
  if let Err(err) = hook_remove_proxy_rule(&resource).await {
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
