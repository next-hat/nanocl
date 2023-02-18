use std::collections::HashMap;

use bollard_next::service::HostConfig;
use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::cargo_config::{CargoConfigPartial, ContainerConfig};

use crate::error::DaemonError;
use crate::models::Pool;
use crate::utils;

fn gen_metrics_cargo(name: &str) -> CargoConfigPartial {
  let mut labels = HashMap::new();
  labels.insert("io.nanocl.cargo".into(), name.into());
  labels.insert("io.nanocl.namespace".into(), "system".into());
  CargoConfigPartial {
    name: name.into(),
    dns_entry: None,
    replication: None,
    container: ContainerConfig {
      image: Some("nexthat/metrsd:v0.1.0".into()),
      labels: Some(labels.to_owned()),
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
  utils::cargo::create("system", cargo, docker_api, pool).await?;
  utils::cargo::start("system-metrics", docker_api).await?;
  Ok(())
}
