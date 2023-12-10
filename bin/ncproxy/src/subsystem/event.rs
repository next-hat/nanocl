use ntex::rt;
use ntex::http;
use futures::StreamExt;
use futures::stream::FuturesUnordered;

use nanocl_error::io::{IoResult, IoError};
use nanocl_error::http_client::HttpClientError;

use nanocl_utils::versioning;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::Event;
use nanocld_client::stubs::resource::ResourcePartial;
use nanocld_client::stubs::system::{EventKind, EventAction};

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
    log::debug!("{err}");
  }
  utils::reload_config(client).await?;
  Ok(())
}

fn get_cargo_attributes(
  attributes: &Option<serde_json::Value>,
) -> IoResult<(String, String)> {
  let attributes = attributes.clone().unwrap_or_default();
  let name = attributes
    .get("Name")
    .map(|e| e.as_str().unwrap_or_default().to_owned())
    .ok_or_else(|| IoError::invalid_data("Attribute Name", "Missing value"))?;
  let namespace_name = attributes
    .get("Namespace")
    .map(|e| e.as_str().unwrap_or_default().to_owned())
    .ok_or_else(|| {
      IoError::invalid_data("Attribute Namespace", "Missing value")
    })?;
  Ok((name, namespace_name))
}

async fn on_event(
  event: &Event,
  nginx: &nginx::Nginx,
  client: &NanocldClient,
) -> IoResult<()> {
  let kind = &event.kind;
  let action = &event.action;
  let actor = event.actor.clone().unwrap_or_default();
  log::debug!("Event {kind} {action}");
  let res = match (kind, action) {
    (EventKind::Cargo, EventAction::Started)
    | (EventKind::Cargo, EventAction::Patched) => {
      let (name, namespace) = get_cargo_attributes(&actor.attributes)?;
      update_cargo_rule(&name, &namespace, nginx, client).await?;
      Ok::<_, IoError>(())
    }
    (EventKind::Cargo, EventAction::Stopped)
    | (EventKind::Cargo, EventAction::Deleted) => {
      let (name, namespace) = get_cargo_attributes(&actor.attributes)?;
      delete_cargo_rule(&name, &namespace, nginx, client).await?;
      Ok::<_, IoError>(())
    }
    (EventKind::Secret, EventAction::Created)
    | (EventKind::Secret, EventAction::Patched) => {
      let resources =
        utils::list_resource_by_secret(&actor.key.unwrap_or_default(), client)
          .await?;
      for resource in resources {
        let resource: ResourcePartial = resource.into();
        update_resource_rule(&resource, nginx, client).await?;
      }
      Ok::<_, IoError>(())
    }
    _ => Ok(()),
  };
  if let Err(err) = res {
    log::debug!("{err}");
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
          let event = match event {
            Err(err) => {
              log::warn!("Unable to get event: {err}");
              continue;
            }
            Ok(event) => event,
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
