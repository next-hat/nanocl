use nanocl_error::http_client::HttpClientError;
use ntex::http;
use futures::StreamExt;
use serde::{Serialize, Deserialize};

use nanocl_utils::versioning;
use nanocl_error::io::{IoResult, FromIo};

use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::Event;
use nanocld_client::stubs::resource::ResourcePartial;

use crate::utils::resource::update_resource_certs;
use crate::version;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProxyRuleCertManagerMetadata {
  pub cert_manager_issuer: Option<String>,
}

async fn on_event(event: Event, client: &NanocldClient) -> IoResult<()> {
  match event {
    //TODO: generate only if necessary
    Event::ResourceCreated(ev) => update_resource_certs(client, &ev).await,
    Event::ResourcePatched(ev) => update_resource_certs(client, &ev).await,
    Event::ResourceDeleted(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      log::info!("received resource deleted event: {ev:#?}");
      Ok(())
    }
    // TODO: create task on secret events
    // Ignore other events
    _ => Ok(()),
  }
}

async fn ensure_resource_config(client: &NanocldClient) -> IoResult<()> {
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

  match client.inspect_resource(&proxy_rule_kind.name).await {
    Ok(_) => {
      if let Err(err) = client
        .put_resource(&proxy_rule_kind.name.clone(), &proxy_rule_kind.into())
        .await
      {
        match err {
          HttpClientError::HttpError(err)
            if err.status == http::StatusCode::CONFLICT =>
          {
            log::info!("CertManagerIssuer already exists. Skipping.")
          }
          _ => {
            log::warn!("Unable to update CertManagerIssuer: {err}");
          }
        }
      }
    }
    Err(_) => {
      if let Err(err) = client.create_resource(&proxy_rule_kind).await {
        match err {
          HttpClientError::HttpError(err)
            if err.status == http::StatusCode::CONFLICT =>
          {
            log::info!("CertManagerIssuer already exists. Skipping.")
          }
          _ => {
            log::warn!("Unable to create CertManagerIssuer: {err}");
          }
        }
      }
    }
  }
  //TODO : scan current resource ssl and generate if needed, add task to renew
  Ok(())
}

pub async fn event_loop(client: &NanocldClient) -> IoResult<()> {
  loop {
    log::info!("Subscribing to nanocl daemon events..");
    match client.watch_events().await {
      Err(err) => {
        log::warn!("Unable to Subscribe to nanocl daemon events: {err}");
      }
      Ok(mut stream) => {
        log::info!("Subscribed to nanocl daemon events");

        ensure_resource_config(client).await?;

        while let Some(event) = stream.next().await {
          let Ok(event) = event else {
            break;
          };
          if let Err(err) = on_event(event, client).await {
            log::warn!("{err}");
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
