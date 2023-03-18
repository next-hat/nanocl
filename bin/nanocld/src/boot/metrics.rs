use bollard_next::service::HostConfig;
use nanocl_stubs::cargo_config::{ContainerConfig, CargoConfigPartial};

use crate::utils;
use crate::models::{Pool, DaemonState};
use crate::error::CliError;

fn gen_metrics_cargo(name: &str) -> CargoConfigPartial {
  CargoConfigPartial {
    name: name.into(),
    replication: None,
    container: ContainerConfig {
      image: Some("nexthat/metrsd:v0.1.0".into()),
      host_config: Some(HostConfig {
        binds: Some(vec!["/run/nanocl:/run/nanocl".into()]),
        ..Default::default()
      }),
      cmd: Some(vec![
        "--hosts".into(),
        "unix:///run/nanocl/metrics.sock".into(),
      ]),
      ..Default::default()
    },
  }
}

pub async fn start_metrics_cargo(state: &DaemonState) -> Result<(), CliError> {
  let cargo = &gen_metrics_cargo("metrics");
  if utils::cargo::inspect("metrics.system", &state)
    .await
    .is_err()
  {
    utils::cargo::create(
      "system",
      cargo,
      format!("v{}", crate::version::VERSION),
      &state.docker_api,
      &state.pool,
    )
    .await?;
    utils::cargo::start("metrics.system", &state.docker_api, &state.pool)
      .await?;
  }
  Ok(())
}
