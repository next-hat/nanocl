use ntex::rt;
use ntex::http;
use futures::StreamExt;
use futures::stream::FuturesUnordered;

use nanocl_utils::versioning;
use nanocl_error::io::{IoResult, IoError};
use nanocl_error::http_client::HttpClientError;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::Event;
use nanocld_client::stubs::resource::ResourcePartial;

use crate::{utils, version, nginx};

/// Update the nginx configuration when a cargo is started, patched
async fn update_cargo_rule(
  name: &str,
  namespace: &str,
  nginx: &nginx::Nginx,
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
  nginx: &nginx::Nginx,
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
  nginx: &nginx::Nginx,
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
  event: &Event,
  nginx: &nginx::Nginx,
  client: &NanocldClient,
) -> IoResult<()> {
  match event {
    Event::CargoStarted(ev) => {
      log::debug!("received cargo started event: {ev:#?}");
      if let Err(err) =
        update_cargo_rule(&ev.name, &ev.namespace_name, nginx, client).await
      {
        log::warn!("{err}");
      }
    }
    Event::CargoPatched(ev) => {
      log::debug!("received cargo patched event: {ev:#?}");
      if let Err(err) =
        update_cargo_rule(&ev.name, &ev.namespace_name, nginx, client).await
      {
        log::warn!("{err}");
      }
    }
    Event::CargoStopped(ev) => {
      log::debug!("received cargo stopped event: {ev:#?}");
      if let Err(err) =
        delete_cargo_rule(&ev.name, &ev.namespace_name, nginx, client).await
      {
        log::warn!("{err}");
      }
    }
    Event::CargoDeleted(ev) => {
      log::debug!("received cargo deleted event: {ev:#?}");
      if let Err(err) =
        delete_cargo_rule(&ev.name, &ev.namespace_name, nginx, client).await
      {
        log::warn!("{err}");
      }
    }
    Event::SecretPatched(secret) => {
      let resources =
        utils::list_resource_by_secret(&secret.key, client).await?;
      for resource in resources {
        let resource: ResourcePartial = resource.into();
        if let Err(err) = update_resource_rule(&resource, nginx, client).await {
          log::warn!("{err}");
        }
      }
    }
    Event::SecretCreated(secret) => {
      let resources =
        utils::list_resource_by_secret(&secret.key, client).await?;
      for resource in resources {
        let resource: ResourcePartial = resource.into();
        if let Err(err) = update_resource_rule(&resource, nginx, client).await {
          log::warn!("{err}");
        }
      }
    }
    // Ignore other events
    _ => {}
  }
  Ok(())
}

async fn ensure_resource_config(client: &NanocldClient) {
  let formated_version = versioning::format_version(version::VERSION);
  let proxy_rule_kind = ResourcePartial {
    kind: "Kind".to_owned(),
    name: "ProxyRule".to_owned(),
    data: serde_json::json!({
      "Url": "unix:///run/nanocl/proxy.sock"
    }),
    version: format!("v{formated_version}"),
    metadata: None,
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

async fn r#loop(nginx: &nginx::Nginx, client: &NanocldClient) {
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
          if let Err(err) = on_event(&event, nginx, client).await {
            log::warn!("{err}");
          }
        }
      }
    }
    log::warn!("Retrying to subscribe in 2 seconds");
    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
  }
}

/// Spawn new thread with event loop to watch for nanocld events
pub(crate) fn spawn(nginx: &nginx::Nginx, client: &NanocldClient) {
  let nginx = nginx.clone();
  let client = client.clone();
  rt::Arbiter::new().exec_fn(move || {
    ntex::rt::spawn(async move {
      r#loop(&nginx, &client).await;
    });
  });
}
