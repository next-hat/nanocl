use ntex::rt;
use futures::StreamExt;

use nanocl_utils::io_error::IoResult;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::Event;
use nanocld_client::stubs::resource::ResourcePartial;

use crate::utils;
use crate::nginx::Nginx;

async fn on_event(
  event: Event,
  nginx: Nginx,
  client: NanocldClient,
) -> IoResult<()> {
  match event {
    Event::CargoStarted(ev) => {
      let resources = utils::list_resource_by_cargo(
        &ev.name,
        Some(ev.namespace_name),
        &client,
      )
      .await?;
      for resource in resources {
        let resource: ResourcePartial = resource.into();
        let proxy_rule = utils::serialize_proxy_rule(&resource)?;
        if let Err(err) = utils::create_resource_conf(
          &resource.name,
          &proxy_rule,
          &client,
          &nginx,
        )
        .await
        {
          log::warn!("{err}");
        }
      }
      utils::reload_config(&client).await?;
    }
    Event::CargoStopped(ev) => {
      let resources = utils::list_resource_by_cargo(
        &ev.name,
        Some(ev.namespace_name),
        &client,
      )
      .await?;
      for resource in resources {
        let resource: ResourcePartial = resource.into();
        nginx.delete_conf_file(&resource.name).await;
      }
      utils::reload_config(&client).await?;
    }
    Event::CargoDeleted(ev) => {
      let resources = utils::list_resource_by_cargo(
        &ev.name,
        Some(ev.namespace_name),
        &client,
      )
      .await?;
      for resource in resources {
        nginx.delete_conf_file(&resource.name).await;
      }
      utils::reload_config(&client).await?;
    }
    Event::ResourceCreated(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      let resource: ResourcePartial = ev.as_ref().clone().into();
      let proxy_rule = utils::serialize_proxy_rule(&resource)?;
      if let Err(err) = utils::create_resource_conf(
        &resource.name,
        &proxy_rule,
        &client,
        &nginx,
      )
      .await
      {
        log::warn!("{err}");
      }
      utils::reload_config(&client).await?;
    }
    Event::ResourcePatched(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      let resource: ResourcePartial = ev.as_ref().clone().into();
      let proxy_rule = utils::serialize_proxy_rule(&resource)?;
      if let Err(err) = utils::create_resource_conf(
        &resource.name,
        &proxy_rule,
        &client,
        &nginx,
      )
      .await
      {
        log::warn!("{err}");
      }
      utils::reload_config(&client).await?;
    }
    Event::ResourceDeleted(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      nginx.delete_conf_file(&ev.name).await;
      utils::reload_config(&client).await?;
    }
    // Ignore other events
    _ => {}
  }
  Ok(())
}

async fn r#loop(client: &NanocldClient, nginx: &Nginx) {
  loop {
    log::info!("Connecting to nanocl daemon...");
    match client.watch_events().await {
      Err(err) => {
        log::warn!("Unable to connect to nanocl daemon got error: {err}");
      }
      Ok(mut stream) => {
        if let Err(err) = utils::sync_resources(client, nginx).await {
          log::warn!("{err}");
        }
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
      "Disconnected from nanocl daemon trying to reconnect in 2 seconds"
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
