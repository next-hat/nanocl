use tokio::fs;

use nanocl_stubs::config::DaemonConfig;

use crate::event;
use crate::models::DaemonState;

use crate::error::CliError;
use crate::version::VERSION;

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
pub async fn init(daemon_conf: &DaemonConfig) -> Result<DaemonState, CliError> {
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
  let daemon_state = DaemonState {
    pool: pool.clone(),
    docker_api: docker_api.clone(),
    config: daemon_conf.to_owned(),
    event_emitter: event::EventEmitter::new(),
    version: VERSION.to_owned(),
  };
  super::system::register_namespace("system", false, &daemon_state).await?;
  super::system::register_namespace("global", true, &daemon_state).await?;
  super::node::register_node(
    &daemon_conf.hostname,
    &daemon_conf.advertise_addr,
    &pool,
  )
  .await?;
  super::system::sync_containers(&docker_api, &pool).await?;
  super::metrics::start_metrics_cargo(&daemon_state).await?;

  Ok(daemon_state)
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
