use ntex::rt;
use ntex::web;
use clap::Parser;
use futures::StreamExt;
use nanocld_client::{
  NanocldClient,
  stubs::{system::Event, resource::ResourcePartial},
};
use nanocl_utils::logger;
use nanocl_utils::io_error::IoResult;

mod cli;
mod nginx;
mod utils;
mod service;
mod network_log;

async fn boot(cli: &cli::Cli) -> IoResult<nginx::Nginx> {
  let nginx = nginx::new(&cli.conf_dir.clone().unwrap_or("/etc/nginx".into()));
  let client = NanocldClient::connect_with_unix_default();

  network_log::run();

  nginx.ensure()?;

  nginx.write_default_conf()?;

  utils::sync_resources(&client, &nginx).await?;

  Ok(nginx)
}

async fn on_event(
  client: NanocldClient,
  nginx: nginx::Nginx,
  event: Event,
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

async fn r#loop(client: &NanocldClient, nginx: &nginx::Nginx) {
  loop {
    log::info!("Connecting to nanocl daemon...");
    match client.watch_events().await {
      Err(err) => {
        log::warn!("Unable to connect to nanocl daemon got error: {err}");
      }
      Ok(mut stream) => {
        log::info!("Connected!");
        while let Some(event) = stream.next().await {
          let Ok(event) = event else {
            break;
          };
          if let Err(err) = on_event(client.clone(), nginx.clone(), event).await
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

fn wait_for_daemon() {
  loop {
    if std::path::Path::new("/run/nanocl/nanocl.sock").exists() {
      break;
    }

    std::thread::sleep(std::time::Duration::from_secs(5));
  }
}

#[ntex::main]
async fn main() -> std::io::Result<()> {
  logger::enable_logger("ncdproxy");

  log::info!("ncdproxy v{}", env!("CARGO_PKG_VERSION"));

  let cli = cli::Cli::parse();

  wait_for_daemon();
  let nginx = boot(&cli).await?;
  let n = nginx.clone();

  rt::Arbiter::new().exec_fn(move || {
    let client = NanocldClient::connect_with_unix_default();
    ntex::rt::spawn(async move {
      r#loop(&client, &n).await;
    });
  });

  let mut server = web::HttpServer::new(move || {
    web::App::new()
      .state(nginx.clone())
      .configure(service::ntex_config)
  });

  server = server.bind_uds("/run/nanocl/proxy.sock")?;

  server.run().await?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use clap::Parser;

  use crate::utils::tests;

  #[ntex::test]
  async fn boot() {
    tests::before();
    let cli =
      super::cli::Cli::parse_from(["ncdproxy", "--conf-dir", "/tmp/nginx"]);

    super::boot(&cli).await.expect("expect to boot");
  }

  #[ntex::test]
  async fn test_scenario() {
    tests::before();
    let res =
      tests::exec_nanocl("nanocl state apply -yf ../tests/test-deploy.yml")
        .await;

    assert!(res.is_ok());

    let res =
      tests::exec_nanocl("nanocl state revert -yf ../tests/test-deploy.yml")
        .await;

    assert!(res.is_ok());
  }
}
