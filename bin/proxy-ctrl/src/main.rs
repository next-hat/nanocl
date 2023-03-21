mod cli;
mod error;
mod nginx;
mod utils;
mod models;

use clap::Parser;
use futures::StreamExt;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::Event;

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
        let proxy_rule = utils::serialize_proxy_rule(&resource)?;
        if let Err(err) =
          nginx.delete_conf_file(&resource.name, &proxy_rule.rule.into())
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
        let proxy_rule = utils::serialize_proxy_rule(&resource)?;
        if let Err(err) =
          nginx.delete_conf_file(&resource.name, &proxy_rule.rule.into())
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
      if let Err(err) = utils::create_resource_conf(&client, &nginx, &ev).await
      {
        err.print();
      }
      utils::reload_config(&client).await?;
    }
    Event::ResourcePatched(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      if let Err(err) = utils::create_resource_conf(&client, &nginx, &ev).await
      {
        err.print();
      }
      utils::reload_config(&client).await?;
    }
    Event::ResourceDeleted(ev) => {
      if ev.kind.as_str() != "ProxyRule" {
        return Ok(());
      }
      let proxy_rule = utils::serialize_proxy_rule(&ev)?;
      nginx.delete_conf_file(&ev.name, &proxy_rule.rule.into())?;
      utils::reload_config(&client).await?;
    }
    // Ignore other events
    _ => {}
  }
  Ok(())
}

async fn r#loop(client: &NanocldClient, nginx: &mut nginx::Nginx) {
  loop {
    log::info!("Connecting to nanocl daemon...");
    match client.watch_events().await {
      Err(err) => {
        log::warn!("Unable to connect to nanocl daemon got error: {err}");
      }
      Ok(mut stream) => {
        log::info!("Connected!");
        if nginx.default_gateway.is_none() {
          if let Ok(info) = client.info().await {
            nginx.set_default_gateway(&info.host_gateway);
          }
        }
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

async fn run(args: cli::Cli) {
  // Build env logger
  if std::env::var("LOG_LEVEL").is_err() {
    std::env::set_var("LOG_LEVEL", "nanocl-ctrl-proxy=info,warn,error,debug");
  }

  let is_test = std::env::var("TEST").is_ok();

  env_logger::Builder::new()
    .parse_env("LOG_LEVEL")
    .format_target(false)
    .is_test(is_test)
    .init();

  log::info!("nanocl-ctrl-proxy v{}", env!("CARGO_PKG_VERSION"));
  let mut nginx =
    nginx::new(&args.conf_dir.to_owned().unwrap_or("/etc/nginx".into()));

  if let Err(err) = nginx.ensure() {
    err.print();
    std::process::exit(1);
  }
  let client = NanocldClient::connect_with_unix_default();
  if let Ok(info) = client.info().await {
    nginx.set_default_gateway(&info.host_gateway);
    if let Err(err) = nginx.write_default_conf() {
      err.print();
    }
  }
  if let Err(err) = utils::sync_resources(&client, &nginx).await {
    err.print();
  }
  drop(args);
  r#loop(&client, &mut nginx).await;
}

fn main() -> std::io::Result<()> {
  ntex::rt::System::new(stringify!("run")).block_on(async {
    let args = cli::Cli::parse();
    run(args).await;
  });
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  use bollard_next::{Docker, container::StartContainerOptions};
  use ntex::rt;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic_scenario() {
    rt::spawn(async move {
      let args = cli::Cli::parse_from(vec![
        "nanocl-ctrl-proxy",
        "--conf-dir",
        "/var/lib/nanocl/proxy",
      ]);
      run(args).await;
    });

    // Deploy a cargo
    let output = exec_nanocl("state apply -y -f ./tests/test-deploy.yml")
      .await
      .expect("Expect to deploy a cargo");
    println!("{output:#?}");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    // Deploy a single http Resource
    let output = exec_nanocl("state apply -y -f ./tests/resource_http.yml")
      .await
      .expect("Expect to deploy a resource");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    // Deploy a private http
    let output =
      exec_nanocl("state apply -y -f ./tests/resource_http_private.yml")
        .await
        .expect("Expect to deploy a resource");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    // Deploy a internal http
    let output =
      exec_nanocl("state apply -y -f ./tests/resource_http_internal.yml")
        .await
        .expect("Expect to deploy a resource");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    // Deploy a public https
    let output = exec_nanocl("state apply -y -f ./tests/resource_https.yml")
      .await
      .expect("Expect to deploy a resource");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    // Stop the cargo
    let output = exec_nanocl("cargo stop get-started")
      .await
      .expect("Expect to stop the cargo");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    // Start the cargo
    let output = exec_nanocl("cargo start get-started")
      .await
      .expect("Expect to start the cargo");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    // ReDeploy a cargo to trigger updates
    let output = exec_nanocl("state apply -y -f ./tests/test-deploy.yml")
      .await
      .expect("Expect to deploy a cargo");
    assert!(output.status.success());

    // Revert deployment
    let output = exec_nanocl("state revert -y -f ./tests/test-deploy.yml")
      .await
      .expect("Expect to revert deployment");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    let output = exec_nanocl("state revert -y -f ./tests/resource_http.yml")
      .await
      .expect("Expect to revert deployment");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    let output = exec_nanocl("state apply -y -f ./tests/resource_tcp.yml")
      .await
      .expect("Expect create a tcp resource");
    assert!(output.status.success());
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;

    let output = exec_nanocl("state revert -y -f ./tests/resource_tcp.yml")
      .await
      .expect("Expect delete a tcp resource");
    assert!(output.status.success());
  }

  #[allow(dead_code)]
  async fn _disconnect_scenario() {
    let fut = rt::spawn(async move {
      let args = cli::Cli::parse_from(vec![
        "nanocl-ctrl-proxy",
        "--conf-dir",
        "/var/lib/nanocl/proxy",
      ]);
      run(args).await;
    });

    let docker_api = Docker::connect_with_unix_defaults().unwrap();

    docker_api
      .stop_container("system-daemon", None)
      .await
      .unwrap();

    let ouput = exec_nanocl("state apply -y -f ./tests/test-deploy.yml")
      .await
      .expect("Expect to deploy a cargo");
    assert_ne!(ouput.status.code(), Some(0));

    docker_api
      .start_container("system-daemon", None::<StartContainerOptions<String>>)
      .await
      .unwrap();

    ntex::time::sleep(std::time::Duration::from_secs(15)).await;
    assert!(!fut.is_finished());

    let ouput = exec_nanocl("state apply -y -f ./tests/test-deploy.yml")
      .await
      .expect("Expect to deploy a cargo");
    assert!(ouput.status.success());

    ntex::time::sleep(std::time::Duration::from_secs(2)).await;

    let output = exec_nanocl("state revert -y -f ./tests/test-deploy.yml")
      .await
      .expect("Expect to revert deployment");
    assert!(output.status.success());
  }
}
