use std::time::Duration;

use ntex::{rt, time::interval};
use futures::StreamExt;

use nanocl_error::io::IoResult;

use metrsd_client::{MetrsdClient, stubs::MetrsdEvent};

use crate::{
  repositories::generic::*,
  models::{Pool, SystemState, MetricDb, MetricNodePartial},
};

/// Save metric event send by [metrsd](http://github.com/next-hat/metrs) to the database
/// The event can be a `CPU`, `MEMORY`, `DISK` or `NETWORK` event.
/// The metric is saved for the current node.
/// This allow us to know what node is the most used.
async fn save_metric(
  node: &str,
  ev: &MetrsdEvent,
  pool: &Pool,
) -> IoResult<()> {
  let node_name = node.to_owned();
  let kind = "nanocl.io/metrs";
  let data = serde_json::to_value(ev)?;
  let mut cpu_percent = ev.cpus.iter().fold(0.0, |acc, cpu| acc + cpu.usage);
  cpu_percent /= ev.cpus.len() as f32;
  let mut memory_percent = ev.memory.used as f32 / ev.memory.total as f32;
  memory_percent *= 100.0;
  let new_cpu_percent = cpu_percent as u32;
  let new_memory_percent = memory_percent as u32;
  let formated_cpu_percent = if new_cpu_percent < 10 {
    format!("0{}", new_cpu_percent)
  } else {
    new_cpu_percent.to_string()
  };
  let formated_memory_percent = if new_memory_percent < 10 {
    format!("0{}", new_memory_percent)
  } else {
    new_memory_percent.to_string()
  };
  let display =
    format!("CPU {formated_cpu_percent}% | MEMORY {formated_memory_percent}%");
  let metric = MetricNodePartial {
    data,
    node_name,
    kind: kind.to_owned(),
    note: Some(display),
  };
  MetricDb::create_from(&metric, pool).await?;
  Ok(())
}

/// Spawn a background thread that will listen to the metrics daemon
/// and save the metrics to the database.
/// The metrics are saved for the current node.
pub(crate) fn spawn(state: &SystemState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(move || {
    let client = MetrsdClient::connect("unix:///run/nanocl/metrics.sock");
    rt::spawn(async move {
      loop {
        log::info!("metrics::spawn_logger: subscribing");
        match client.subscribe().await {
          Ok(mut stream) => {
            log::info!("metrics::spawn_logger: subcribed");
            while let Some(res) = stream.next().await {
              match res {
                Ok(ev) => {
                  if let Err(err) =
                    save_metric(&state.config.hostname, &ev, &state.pool).await
                  {
                    log::warn!("metrics::spawn_logger: {err}");
                  }
                }
                Err(err) => {
                  log::error!("metrics::spawn_logger: {err}");
                  break;
                }
              }
            }
          }
          Err(err) => {
            log::warn!("metrics::spawn_logger: {err}")
          }
        }
        log::warn!("metrics::spawn_logger: reconnecting in 2 seconds...");
        interval(Duration::from_secs(2)).tick().await;
      }
    });
  });
}
