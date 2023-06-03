use ntex::rt;
use ntex::http;
use futures::StreamExt;
use futures::stream::FuturesUnordered;

use nanocl_utils::versioning;
use nanocl_utils::io_error::{IoResult, IoError};
use nanocl_utils::http_client_error::HttpClientError;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::Event;
use nanocld_client::stubs::resource::ResourcePartial;

use crate::utils;
use crate::version;
use crate::nginx::Nginx;

/// Update the nginx configuration when a cargo is started, patched
async fn update_cargo_rule(
  name: &str,
  namespace: &str,
  nginx: &Nginx,
  client: &NanocldClient,
) -> IoResult<()> {
  let resources =
    utils::list_resource_by_cargo(name, Some(namespace.to_owned()), client)
      .await?;
  resources
    .into_iter()
    .map(|resource| async {
      let resource: ResourcePartial = resource.into();
      let proxy_rule = utils::serialize_proxy_rule(&resource)?;
      if let Err(err) =
        utils::create_resource_conf(&resource.name, &proxy_rule, client, nginx)
          .await
      {
        log::warn!("{err}");
      }
      Ok::<_, IoError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, IoError>>()?;
  utils::reload_config(client).await?;
  Ok(())
}

/// Update the nginx configuration when a cargo is stopped, deleted
async fn delete_cargo_rule(
  name: &str,
  namespace: &str,
  nginx: &Nginx,
  client: &NanocldClient,
) -> IoResult<()> {
  let resources =
    utils::list_resource_by_cargo(name, Some(namespace.to_owned()), client)
      .await?;
  resources
    .into_iter()
    .map(|resource| async {
      let resource: ResourcePartial = resource.into();
      nginx.delete_conf_file(&resource.name).await;
      Ok::<_, IoError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, IoError>>()?;
  utils::reload_config(client).await?;
  Ok(())
}

/// Update the nginx configuration when a resource is created, patched
async fn update_resource_rule(
  resource: &ResourcePartial,
  nginx: &Nginx,
  client: &NanocldClient,
) -> IoResult<()> {
  let proxy_rule = utils::serialize_proxy_rule(resource)?;
  if let Err(err) =
    utils::create_resource_conf(&resource.name, &proxy_rule, client, nginx)
      .await
  {
    log::warn!("{err}");
  }
  utils::reload_config(client).await?;
  Ok(())
}

async fn on_event(
  event: Event,
  nginx: Nginx,
  client: NanocldClient,
) -> IoResult<()> {
  match event {
    Event::CargoStarted(ev) => {
      log::debug!("received cargo started event: {ev:#?}");
      if let Err(err) =
        update_cargo_rule(&ev.name, &ev.namespace_name, &nginx, &client).await
      {
        log::warn!("{err}");
      }
    }
    Event::CargoPatched(ev) => {
      log::debug!("received cargo patched event: {ev:#?}");
      if let Err(err) =
        update_cargo_rule(&ev.name, &ev.namespace_name, &nginx, &client).await
      {
        log::warn!("{err}");
      }
    }
    Event::CargoStopped(ev) => {
      log::debug!("received cargo stopped event: {ev:#?}");
      if let Err(err) =
        delete_cargo_rule(&ev.name, &ev.namespace_name, &nginx, &client).await
      {
        log::warn!("{err}");
      }
    }
    Event::CargoDeleted(ev) => {
      log::debug!("received cargo deleted event: {ev:#?}");
      if let Err(err) =
        delete_cargo_rule(&ev.name, &ev.namespace_name, &nginx, &client).await
      {
        log::warn!("{err}");
      }
    }
    Event::ResourceCreated(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      log::debug!("received resource created event: {ev:#?}");
      let resource: ResourcePartial = ev.as_ref().clone().into();
      if let Err(err) = update_resource_rule(&resource, &nginx, &client).await {
        log::warn!("{err}");
      }
    }
    Event::ResourcePatched(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      log::debug!("received resource patched event: {ev:#?}");
      let resource: ResourcePartial = ev.as_ref().clone().into();
      if let Err(err) = update_resource_rule(&resource, &nginx, &client).await {
        log::warn!("{err}");
      }
    }
    Event::ResourceDeleted(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      log::debug!("received resource deleted event: {ev:#?}");
      nginx.delete_conf_file(&ev.name).await;
      utils::reload_config(&client).await?;
    }
    // Ignore other events
    _ => {}
  }
  Ok(())
}

async fn ensure_resource_config(client: &NanocldClient) {
  let formated_version = versioning::format_version(version::VERSION);
  let proxy_rule_kind = ResourcePartial {
    kind: "Kind".to_string(),
    name: "ProxyRule".to_string(),
    config: serde_json::json!({
      "Url": "unix:///run/nanocl/proxy.sock"
    }),
    version: format!("v{formated_version}"),
  };

  match client.inspect_resource(&proxy_rule_kind.name).await {
    Ok(_) => {
      if let Err(err) = client
        .put_resource(&proxy_rule_kind.name, &proxy_rule_kind.clone().into())
        .await
      {
        match err {
          HttpClientError::HttpError(err)
            if err.status == http::StatusCode::CONFLICT =>
          {
            log::info!("ProxyRule already exists. Skipping.")
          }
          _ => {
            log::warn!("Unable to update ProxyRule: {err}");
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
            log::info!("ProxyRule already exists. Skipping.")
          }
          _ => {
            log::warn!("Unable to create ProxyRule: {err}");
          }
        }
      }
    }
  }
}

async fn r#loop(client: &NanocldClient, nginx: &Nginx) {
  loop {
    log::info!("Subscribing to nanocl daemon events..");
    match client.watch_events().await {
      Err(err) => {
        log::warn!("Unable to Subscribe to nanocl daemon events: {err}");
      }
      Ok(mut stream) => {
        log::info!("Subscribed to nanocl daemon events");

        ensure_resource_config(client).await;

        while let Some(event) = stream.next().await {
          let Ok(event) = event else {
            break;
          };
          if let Err(err) = on_event(event, nginx.clone(), client.clone()).await
          {
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

/// Spawn new thread with event loop to watch for nanocld events
pub(crate) fn spawn(nginx: &Nginx) {
  let nginx = nginx.clone();
  rt::Arbiter::new().exec_fn(move || {
    let client = NanocldClient::connect_with_unix_default();
    ntex::rt::spawn(async move {
      r#loop(&client, &nginx).await;
    });
  });
}
