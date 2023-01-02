use bollard::Docker;

use nanocl_models::config::DaemonConfig;

use super::Pool;

pub struct DaemonState {
  pub(crate) pool: Pool,
  pub(crate) docker_api: Docker,
  pub(crate) config: DaemonConfig,
}

pub struct ArgState {
  pub(crate) config: DaemonConfig,
  pub(crate) pool: Pool,
  pub(crate) default_namespace: String,
  pub(crate) sys_namespace: String,
}
