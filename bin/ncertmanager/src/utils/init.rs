use nanocl_error::http_client::HttpClientError;
use nanocld_client::{
  stubs::secret::{SecretQuery},
  NanocldClient,
};
use ntex::{http};

use nanocl_utils::versioning;
use nanocl_error::io::{IoResult, FromIo};

use nanocld_client::stubs::resource::{ResourcePartial, ResourceQuery};

use crate::{version, manager::NCertManager};

use super::event::{handle_resource_update, handle_secret_update};

fn display_resource_kind_error(err: HttpClientError) -> IoResult<()> {
  match err {
    HttpClientError::HttpError(err)
      if err.status == http::StatusCode::CONFLICT =>
    {
      log::info!("CertManagerIssuer already exists. Skipping.");
      Ok(())
    }
    _ => {
      log::warn!("Unable to update CertManagerIssuer: {err}");
      Err(err.map_err_context(|| "Resource kind creation").into())
    }
  }
}

async fn create_resource_kind<'a>(manager: &NCertManager<'a>) -> IoResult<()> {
  let cargo_config_schema_bytes = include_bytes!("../../cargo_config.json");

  let formated_version = versioning::format_version(version::VERSION);

  let data =
    serde_json::from_slice::<serde_json::Value>(cargo_config_schema_bytes)
      .map_err(|err| err.map_err_context(|| "Infos"))?;

  let proxy_rule_kind = ResourcePartial {
    kind: "Kind".to_owned(),
    name: "CertManagerIssuer".to_owned(),
    data,
    version: format!("v{formated_version}"),
    metadata: None,
  };

  match manager.client.inspect_resource(&proxy_rule_kind.name).await {
    Ok(_) => {
      if let Err(err) = manager
        .client
        .put_resource(&proxy_rule_kind.name.clone(), &proxy_rule_kind.into())
        .await
      {
        display_resource_kind_error(err)?
      }
    }
    Err(_) => {
      if let Err(err) = manager.client.create_resource(&proxy_rule_kind).await {
        display_resource_kind_error(err)?
      }
    }
  }

  Ok(())
}

async fn handle_current_resources<'a>(
  manager: &NCertManager<'a>,
) -> IoResult<()> {
  let managed_resources = manager
    .client
    .list_resource(Some(&ResourceQuery {
      meta_exists: Some("CertManagerIssuer".to_owned()),
      ..Default::default()
    }))
    .await?
    .into_iter();

  for resource in managed_resources {
    let name = resource.name.to_owned();
    let handle_res = handle_resource_update(manager, resource).await;
    if let Err(err) = handle_res {
      log::error!("Can't update resource {name}: {err}");
    }
  }

  Ok(())
}

pub async fn handle_current_secrets<'a>(
  manager: &mut NCertManager<'a>,
) -> IoResult<()> {
  let managed_secrets = manager
    .client
    .list_secret(Some(&SecretQuery {
      meta_exists: Some("CertManagerIssuer".to_owned()),
      ..Default::default()
    }))
    .await?
    .into_iter();

  for secret in managed_secrets {
    let key = secret.key.to_owned();

    if let Err(err) = handle_secret_update(manager, secret).await {
      log::error!("Can't update secret {key}: {err}");
    }
  }

  Ok(())
}

pub async fn init_cert_manager(
  client: &NanocldClient,
  cert_dir: String,
) -> IoResult<NCertManager> {
  let infos = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Infos"))?;

  let mut manager =
    NCertManager::new(client, infos.config.state_dir.to_owned(), cert_dir);
  create_resource_kind(&manager).await?;
  handle_current_resources(&manager).await?;
  handle_current_secrets(&mut manager).await?;

  Ok(manager)
}
