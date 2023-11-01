use nanocl_error::http_client::HttpClientError;
use nanocld_client::stubs::proxy::ProxySslConfig;
use nanocld_client::stubs::secret::{Secret, SecretQuery};
use ntex::{http, rt, channel};
use futures::{StreamExt, FutureExt, stream, select};
use ntex_util::future::{Either};
use openssl::asn1::Asn1Time;
use serde::{Serialize, Deserialize};

use nanocl_utils::versioning;
use nanocl_error::io::{IoResult, FromIo};

use nanocld_client::stubs::system::Event;
use nanocld_client::stubs::resource::{ResourcePartial, Resource, ResourceQuery};

use crate::manager::NCertManager;
use crate::utils::resource::update_resource_certs;
use crate::utils::secret::{SecretMetadata, get_expiry_time};
use crate::version;

const CHECK_RENEW_DELAY: u64 = 2;
// const CHECK_RENEW_DELAY: u64 = 60 * 60 * 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProxyRuleCertManagerMetadata {
  pub cert_manager_issuer: String,
}

pub async fn handle_resource_update(
  manager: &NCertManager,
  resource: Resource,
) -> IoResult<()> {
  if resource.kind.as_str() != "ProxyRule" {
    return Ok(());
  }

  match &resource.metadata {
    Some(metadata) => {
      let metadata = serde_json::from_value::<ProxyRuleCertManagerMetadata>(
        metadata.clone(),
      )
      .map_err(|err| err.map_err_context(|| "ProxyRule metadata parsing"))?;

      let issuer_key = metadata.cert_manager_issuer;

      log::info!("Resource handling {resource:#?}");

      update_resource_certs(&manager.client, resource, issuer_key).await
    }
    None => Ok(()),
  }
}

pub async fn handle_secret_update(
  manager: &mut NCertManager,
  secret: Secret,
) -> IoResult<()> {
  if secret.kind.as_str() != "Tls" {
    return Ok(());
  }
  match &secret.metadata {
    Some(metadata) => {
      serde_json::from_value::<SecretMetadata>(metadata.clone())
        .map_err(|err| err.map_err_context(|| "ProxyRule metadata parsing"))?;

      let expiry = get_expiry_time(&secret)?;

      if NCertManager::is_renew_date_past(&expiry).unwrap() {
        log::warn!("Expiration date to soon");
      }

      manager.add_secret(secret.key, expiry);
      log::info!("cert added to tasks");

      manager.debug();

      Ok(())
    }
    None => Ok(()),
  }
}

async fn on_event(event: Event, manager: &mut NCertManager) -> IoResult<()> {
  match event {
    Event::ResourceCreated(ev) => handle_resource_update(manager, *ev).await,
    Event::ResourcePatched(ev) => handle_resource_update(manager, *ev).await,
    Event::SecretCreated(ev) => handle_secret_update(manager, *ev).await,
    Event::SecretPatched(ev) => handle_secret_update(manager, *ev).await,
    _ => Ok(()),
  }
}

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

async fn create_resource_kind(manager: &NCertManager) -> IoResult<()> {
  let cargo_config_schema_bytes = include_bytes!("../cargo_config.json");

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

async fn handle_current_resources(manager: &NCertManager) -> IoResult<()> {
  let managed_resources = manager
    .client
    .list_resource(Some(ResourceQuery {
      meta_exists: Some("cert_manager_issuer".to_owned()),
      ..Default::default()
    }))
    .await?
    .into_iter();

  for resource in managed_resources {
    handle_resource_update(manager, resource).await?
  }

  Ok(())
}

async fn handle_current_secrets(manager: &NCertManager) -> IoResult<()> {
  let managed_resources = manager
    .client
    .list_secret(Some(SecretQuery {
      meta_exists: Some("cert_manager_issuer".to_owned()),
      ..Default::default()
    }))
    .await?
    .into_iter();

  for resource in managed_resources {
    handle_resource_update(manager, resource).await?
  }

  Ok(())
}

async fn ensure_resource_config(manager: &NCertManager) -> IoResult<()> {
  create_resource_kind(manager).await?;
  handle_current_resources(manager).await?;
  handle_current_secrets(manager).await?;
  Ok(())
}

async fn get_renew_timer() {
  ntex::time::sleep(std::time::Duration::from_secs(CHECK_RENEW_DELAY)).await;
}

pub async fn event_loop(manager: &mut NCertManager) -> IoResult<()> {
  loop {
    log::info!("Subscribing to nanocl daemon events..");

    match manager.client.watch_events().await {
      Err(err) => {
        log::warn!("Unable to Subscribe to nanocl daemon events: {err}");
      }
      Ok(stream) => {
        log::info!("Subscribed to nanocl daemon events");

        let (sx, rx) = channel::mpsc::channel::<Option<bool>>();

        rt::spawn(async move {
          loop {
            get_renew_timer().await;
            if let Err(err) = sx.send(Some(true)) {
              log::warn!("Sx error {err}");
            }
          }
        });

        ensure_resource_config(manager).await?;

        let mut selected_streams = futures::stream::select(
          stream.map(Either::Left),
          rx.map(Either::Right),
        );

        while let Some(res) = selected_streams.next().await {
          match res {
            Either::Right(_) => {
              log::info!("Renew");
            }
            Either::Left(Ok(event)) => {
              if let Err(err) = on_event(event, manager).await {
                log::warn!("{err}");
              }
            }
            Either::Left(Err(_)) => break,
          }
        }
      }
    }

    log::warn!(
      "Unsubscribed from nanocl daemon events, retrying to subscribe in 2 seconds"
    );

    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
  }
}
