use std::time::Duration;

use ntex::{rt, time::interval};
use futures::StreamExt;

use nanocl_error::io::IoResult;

use metrsd_client::{MetrsdClient, MetrsdEvent};

use crate::{
  repositories::generic::*,
  models::{Pool, DaemonState, MetricDb, MetricNodePartial},
};

/// Save metric event send by [metrsd](http://github.com/next-hat/metrsd) to the database
/// The event can be a `CPU`, `MEMORY`, `DISK` or `NETWORK` event.
/// The metric is saved for the current node.
/// This allow us to know what node is the most used.
async fn save_metric(
  node: &str,
  ev: &MetrsdEvent,
  pool: &Pool,
) -> IoResult<()> {
  let node_name = node.to_owned();
  let metric = match ev {
    MetrsdEvent::Cpu(cpus) => ("nanocl.io/cpu", serde_json::to_value(cpus)?),
    MetrsdEvent::Memory(mem) => {
      ("nanocl.io/memory", serde_json::to_value(mem)?)
    }
    MetrsdEvent::Disk(disk) => ("nanocl.io/disk", serde_json::to_value(disk)?),
    MetrsdEvent::Network(net) => {
      ("nanocl.io/network", serde_json::to_value(net)?)
    }
  };
  let metric = MetricNodePartial {
    node_name,
    kind: metric.0.to_owned(),
    data: metric.1,
  };
  MetricDb::create_from(&metric, pool).await?;
  Ok(())
}

/// Spawn a background thread that will listen to the metrics daemon
/// and save the metrics to the database.
/// The metrics are saved for the current node.
pub(crate) fn spawn_logger(state: &DaemonState) {
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
