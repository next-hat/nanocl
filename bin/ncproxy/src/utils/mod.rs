pub mod nginx;
pub mod resource;
pub mod rule;
pub mod server;

#[cfg(test)]
pub(crate) mod tests {
  use std::sync::Arc;

  use bollard_next::container::Config;
  use nanocl_error::io::{FromIo, IoResult};
  pub use nanocl_utils::ntex::test_client::*;

  use nanocld_client::{
    ConnectOpts, NanocldClient,
    stubs::{
      cargo::CargoDeleteQuery, cargo_spec::CargoSpecPartial,
      proxy::ResourceProxyRule,
    },
  };

  use crate::{services, vars};

  pub fn before() {
    // Set test env to true to setup test logger
    unsafe {
      std::env::set_var("TEST", "true");
    }
  }

  pub async fn ensure_test_cargo() -> IoResult<()> {
    const CARGO_NAME: &str = "ncproxy-test";
    const CARGO_KEY: &str = "global.ncproxy-test";
    const CARGO_IMAGE: &str = "ghcr.io/next-hat/nanocl-get-started:latest";
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".to_owned(),
      ..Default::default()
    })?;
    if client.inspect_cargo(CARGO_KEY).await.is_err() {
      let cargo = CargoSpecPartial {
        name: CARGO_NAME.to_owned(),
        container: Config {
          image: Some(CARGO_IMAGE.to_owned()),
          ..Default::default()
        },
        ..Default::default()
      };
      client.create_cargo(&cargo, None).await?;
    }
    client.start_process("cargo", CARGO_KEY).await?;
    Ok(())
  }

  pub async fn clean_test_cargo() -> IoResult<()> {
    const CARGO_KEY: &str = "global.ncproxy-test";
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })?;
    if client.inspect_cargo(CARGO_KEY).await.is_err() {
      return Ok(());
    }
    client
      .delete_cargo(CARGO_KEY, Some(&CargoDeleteQuery { force: Some(true) }))
      .await?;
    Ok(())
  }

  pub fn read_rule(path: &str) -> IoResult<ResourceProxyRule> {
    let resource = std::fs::read_to_string(path)?;
    let rule: ResourceProxyRule =
      serde_yaml::from_str(&resource).map_err(|err| {
        err.map_err_context(|| format!("Failed to parse rule: {}", path))
      })?;
    Ok(rule)
  }

  pub async fn gen_default_test_client() -> TestClient {
    before();
    let home = std::env::var("HOME").unwrap();
    let options = crate::cli::Cli {
      state_dir: format!("{home}/.nanocl_dev/state/proxy"),
      nginx_dir: "/etc/nginx".to_owned(),
    };
    let system_state = crate::subsystem::init(&options).await.unwrap();
    // Create test server
    let srv = ntex::web::test::server(async move || {
      ntex::web::App::new()
        .state(Arc::clone(&system_state))
        .configure(services::ntex_config)
    })
    .await;
    TestClient::new(srv, vars::VERSION)
  }
}
