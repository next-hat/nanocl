use nanocl_stubs::config::DaemonConfig;

use crate::event;
use crate::models::BootState;

use crate::error::DaemonError;

/// Init function called before http server start
/// to initialize our state
pub async fn init(
  daemon_conf: &DaemonConfig,
) -> Result<BootState, DaemonError> {
  let docker_api = bollard_next::Docker::connect_with_unix(
    &daemon_conf.docker_host,
    120,
    bollard_next::API_DEFAULT_VERSION,
  )?;
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
      hostname: None,
    };

    let config = config::init(&args)?;

    // test function init
    let _ = init(&config).await?;

    Ok(())
  }
}
