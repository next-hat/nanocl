use nanocl_stubs::config::DaemonConfig;
use tokio::fs;

use crate::event;
use crate::models::BootState;

use crate::error::CliError;

pub async fn ensure_state_dir(state_dir: &str) -> Result<(), CliError> {
  let vm_dir = format!("{state_dir}/vms/images");
  fs::create_dir_all(vm_dir).await.map_err(|err| {
    CliError::new(
      1,
      format!("Unable to create state directory {state_dir}: {err}"),
    )
  })?;
  Ok(())
}

/// Init function called before http server start
/// to initialize our state
pub async fn init(daemon_conf: &DaemonConfig) -> Result<BootState, CliError> {
  let docker_api = bollard_next::Docker::connect_with_unix(
    &daemon_conf.docker_host,
    120,
    bollard_next::API_DEFAULT_VERSION,
  )
  .map_err(|err| {
    CliError::new(
      1,
      format!(
        "Error while connecting to docker at {}: {}",
        err, &daemon_conf.docker_host
      ),
    )
  })?;
  ensure_state_dir(&daemon_conf.state_dir).await?;
  super::system::ensure_network("system", &docker_api).await?;
  let pool = super::store::ensure(daemon_conf, &docker_api).await?;
  super::system::register_namespace("system", false, &docker_api, &pool)
    .await?;
  super::system::register_namespace("global", true, &docker_api, &pool).await?;
  super::node::register_node(
    &daemon_conf.hostname,
    &daemon_conf.gateway,
    &pool,
  )
  .await?;
  super::system::sync_containers(&docker_api, &pool).await?;
  super::metrics::start_metrics_cargo(&docker_api, &pool).await?;
  Ok(BootState {
    pool,
    docker_api,
    config: daemon_conf.to_owned(),
    event_emitter: event::EventEmitter::new(),
  })
}

/// Init unit test
#[cfg(test)]
mod tests {
  use super::*;

  use crate::config;
  use crate::cli::Cli;
  use crate::utils::tests::*;

  /// Test init
  #[ntex::test]
  async fn basic_init() -> TestRet {
    // Init cli args
    let args = Cli {
      init: false,
      hosts: None,
      docker_host: None,
      state_dir: None,
      conf_dir: String::from("/etc/nanocl"),
      gateway: None,
      nodes: Vec::default(),
      hostname: None,
      advertise_addr: None,
    };

    let config = config::init(&args).expect("Expect to init config");

    // test function init
    let _ = init(&config).await.expect("Expect to init state");

    Ok(())
  }
}
