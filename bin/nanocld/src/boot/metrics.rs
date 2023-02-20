use bollard_next::service::HostConfig;
use nanocl_stubs::cargo_config::{CargoConfigPartial, ContainerConfig};

use crate::error::DaemonError;
use crate::models::Pool;
use crate::utils;

fn gen_metrics_cargo(name: &str) -> CargoConfigPartial {
  CargoConfigPartial {
    name: name.into(),
    dns_entry: None,
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

pub async fn start_metrics_cargo(
  docker_api: &bollard_next::Docker,
  pool: &Pool,
) -> Result<(), DaemonError> {
  let cargo = &gen_metrics_cargo("metrics");
  if utils::cargo::inspect("system-metrics", docker_api, pool)
    .await
    .is_err()
  {
    utils::cargo::create("system", cargo, docker_api, pool).await?;
    utils::cargo::start("system-metrics", docker_api).await?;
  }
  Ok(())
}
