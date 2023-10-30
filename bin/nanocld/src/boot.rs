use tokio::fs;
use nanocl_error::io::{FromIo, IoResult};

use nanocl_stubs::config::DaemonConfig;

use crate::{event, utils};
use crate::models::DaemonState;

use crate::version::VERSION;

/// ## Ensure state dir
///
/// Ensure that the state dir exists and is ready to use
///
/// ## Arguments
///
/// * [state_dir](str) - The state dir path
///
/// ## Returns
///
/// * [IoResult](IoResult<()>) - The result of the operation
///
async fn ensure_state_dir(state_dir: &str) -> IoResult<()> {
  let vm_dir = format!("{state_dir}/vms/images");
  fs::create_dir_all(vm_dir).await.map_err(|err| {
    err.map_err_context(|| format!("Unable to create {state_dir}/vms/images"))
  })?;
  Ok(())
}

/// ## Init
///
/// Init function called before http server start.
/// To boot and initialize our state and database.
///
/// ## Arguments
///
/// * [daemon_conf](DaemonConfig) - The daemon configuration
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](DaemonState) - The daemon state
///   * [Err](IoError) - The daemon state has not been initialized
///
pub async fn init(daemon_conf: &DaemonConfig) -> IoResult<DaemonState> {
  let docker = bollard_next::Docker::connect_with_unix(
    &daemon_conf.docker_host,
    120,
    bollard_next::API_DEFAULT_VERSION,
  )
  .map_err(|err| {
    err.map_err_context(|| "Unable to connect to docker daemon")
  })?;
  ensure_state_dir(&daemon_conf.state_dir).await?;
  let pool = utils::store::init().await?;
  let daemon_state = DaemonState {
    pool: pool.clone(),
    docker_api: docker.clone(),
    config: daemon_conf.to_owned(),
    event_emitter: event::EventEmitter::new(),
    version: VERSION.to_owned(),
  };
  utils::system::register_namespace("system", false, &daemon_state).await?;
  utils::system::register_namespace("global", true, &daemon_state).await?;
  utils::system::sync_containers(&docker, &pool).await?;
  utils::system::sync_vm_images(daemon_conf, &pool).await?;
  Ok(daemon_state)
}

/// Init unit test
#[cfg(test)]
mod tests {
  use super::*;

  use crate::config;
  use crate::cli::Cli;

  /// Test init
  #[ntex::test]
  async fn basic_init() {
    // Init cli args
    let args = Cli {
      gid: 0,
      init: false,
      hosts: None,
      docker_host: None,
      state_dir: Some(String::from("/tmp/nanocl")),
      conf_dir: String::from("/etc/nanocl"),
      gateway: None,
      nodes: Vec::default(),
      hostname: None,
      advertise_addr: None,
    };
    let config = config::init(&args).expect("Expect to init config");
    // test function init
    let _ = init(&config).await.unwrap();
  }
}
