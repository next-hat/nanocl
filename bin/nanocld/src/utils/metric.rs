use std::time::Duration;

use ntex::rt;
use futures::StreamExt;
use ntex::time::interval;
use metrsd_client::{MetrsdClient, MetrsdEvent};

use crate::models::{Pool, MetricPartial, DaemonState, MetricDb, Repository};

/// Save metric event send by [metrsd](http://github.com/nxthat/metrsd) to the database
/// The event can be a `CPU`, `MEMORY`, `DISK` or `NETWORK` event.
/// The metric is saved for the current node.
/// This allow us to know what node is the most used.
async fn save_metric(node: &str, ev: &MetrsdEvent, pool: &Pool) {
  match ev {
    MetrsdEvent::Cpu(cpus) => {
      let item = MetricPartial {
        kind: "CPU".into(),
        node_name: node.to_owned(),
        data: serde_json::to_value(cpus).unwrap(),
      };
      let _ = MetricDb::create(&item, pool).await;
    }
    MetrsdEvent::Memory(mem) => {
      let item = MetricPartial {
        kind: "MEMORY".into(),
        node_name: node.to_owned(),
        data: serde_json::to_value(mem).unwrap(),
      };
      let _ = MetricDb::create(&item, pool).await;
    }
    MetrsdEvent::Disk(disk) => {
      let item = MetricPartial {
        kind: "DISK".into(),
        node_name: node.to_owned(),
        data: serde_json::to_value(disk).unwrap(),
      };
      let _ = MetricDb::create(&item, pool).await;
    }
    MetrsdEvent::Network(net) => {
      let item = MetricPartial {
        kind: "NETWORK".into(),
        node_name: node.to_owned(),
        data: serde_json::to_value(net).unwrap(),
      };
      let _ = MetricDb::create(&item, pool).await;
    }
  }
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
        match client.subscribe().await {
          Ok(mut stream) => {
            while let Some(res) = stream.next().await {
              match res {
                Ok(ev) => {
                  save_metric(&state.config.hostname, &ev, &state.pool).await;
                }
                Err(err) => {
                  log::error!("Error while receiving metric : {}", err);
                  break;
                }
              }
            }
          }
          Err(err) => {
            log::warn!("Error while connecting to metrics daemon : {err}")
          }
        }
        log::warn!("Reconnecting to metrics daemon in 2 seconds...");
        interval(Duration::from_secs(2)).tick().await;
      }
    });
  });
}
