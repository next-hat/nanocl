mod cli;
mod error;
mod nginx;
mod utils;
mod service;

use clap::Parser;

use futures::StreamExt;
use ntex::{
  web::{App, HttpServer},
  rt,
};
use nanocld_client::{
  NanocldClient,
  stubs::{system::Event, resource::ResourcePartial},
};

async fn boot(cli: &cli::Cli) -> nginx::Nginx {
  if std::env::var("LOG_LEVEL").is_err() {
    std::env::set_var("LOG_LEVEL", "nanocl-ncdproxy=info,warn,error,debug");
  }
  let is_test = std::env::var("TEST").is_ok();
  env_logger::Builder::new()
    .parse_env("LOG_LEVEL")
    .format_target(false)
    .is_test(is_test)
    .init();

  log::info!("nanocl-ncdproxy v{}", env!("CARGO_PKG_VERSION"));

  let nginx = nginx::new(&cli.conf_dir.clone().unwrap_or("/etc/nginx".into()));
  let client = NanocldClient::connect_with_unix_default();

  if let Err(err) = nginx.ensure() {
    err.exit();
  }

  if let Err(err) = nginx.write_default_conf() {
    err.exit();
  }

  if let Err(err) = utils::sync_resources(&client, &nginx).await {
    err.exit();
  }

  nginx
}

async fn on_event(
  client: NanocldClient,
  nginx: nginx::Nginx,
  event: Event,
) -> Result<(), error::ErrorHint> {
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
        if let Err(err) =
          utils::create_resource_conf(&client, &nginx, &resource).await
        {
          err.print();
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
        let proxy_rule = utils::serialize_proxy_rule(&resource)?;
        if let Err(err) = nginx
          .delete_conf_file(&resource.name, &proxy_rule.rule.into())
          .await
        {
          err.print();
        }
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
        let resource: ResourcePartial = resource.into();
        let proxy_rule = utils::serialize_proxy_rule(&resource)?;
        if let Err(err) = nginx
          .delete_conf_file(&resource.name, &proxy_rule.rule.into())
          .await
        {
          err.print();
        }
      }
      utils::reload_config(&client).await?;
    }
    Event::ResourceCreated(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      let resource: ResourcePartial = ev.as_ref().clone().into();
      if let Err(err) =
        utils::create_resource_conf(&client, &nginx, &resource).await
      {
        err.print();
      }
      utils::reload_config(&client).await?;
    }
    Event::ResourcePatched(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      let resource: ResourcePartial = ev.as_ref().clone().into();
      if let Err(err) =
        utils::create_resource_conf(&client, &nginx, &resource).await
      {
        err.print();
      }
      utils::reload_config(&client).await?;
    }
    Event::ResourceDeleted(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      let resource: ResourcePartial = ev.as_ref().clone().into();
      let proxy_rule = utils::serialize_proxy_rule(&resource)?;
      let _ = nginx
        .delete_conf_file(&ev.name, &proxy_rule.rule.into())
        .await;
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
            err.print();
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

fn main() -> std::io::Result<()> {
  ntex::rt::System::new(stringify!("run")).block_on(async move {
    let cli = cli::Cli::parse();

    wait_for_daemon();
    let nginx = boot(&cli).await;
    let n = nginx.clone();

    rt::Arbiter::new().exec_fn(move || {
      let client = NanocldClient::connect_with_unix_default();
      ntex::rt::spawn(async move {
        r#loop(&client, &n).await;
      });
    });

    let mut server = HttpServer::new(move || {
      App::new()
        .state(nginx.clone())
        .configure(service::ntex_config)
    });

    server = server.bind_uds("/run/nanocl/proxy.sock")?;

    server.run().await?;
    Ok::<_, std::io::Error>(())
  })?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use clap::Parser;

  use crate::utils::tests;

  #[ntex::test]
  async fn boot() {
    let cli =
      super::cli::Cli::parse_from(["ncdproxy", "--conf-dir", "/tmp/nginx"]);

    super::boot(&cli).await;
  }

  #[ntex::test]
  async fn test_scenario() {
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
