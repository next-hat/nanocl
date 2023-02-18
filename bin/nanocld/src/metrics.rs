use std::time::Duration;

use ntex::rt;
use futures::StreamExt;
use ntex::time::interval;
use metrsd_client::{MetrsdClient, MetrsdEvent};

use crate::repositories::metrics;
use crate::models::{Pool, MetricInsertDbModel};

async fn save_metric(pool: &Pool, ev: MetrsdEvent) {
  match ev {
    MetrsdEvent::Cpu(cpus) => {
      let item = MetricInsertDbModel {
        kind: "CPU".into(),
        data: serde_json::to_value(&cpus).unwrap(),
      };
      let _ = metrics::create(item, pool).await;
    }
    MetrsdEvent::Memory(mem) => {
      let item = MetricInsertDbModel {
        kind: "MEMORY".into(),
        data: serde_json::to_value(&mem).unwrap(),
      };
      let _ = metrics::create(item, pool).await;
    }
    MetrsdEvent::Disk(disk) => {
      let item = MetricInsertDbModel {
        kind: "DISK".into(),
        data: serde_json::to_value(&disk).unwrap(),
      };
      let _ = metrics::create(item, pool).await;
    }
    MetrsdEvent::Network(net) => {
      let item = MetricInsertDbModel {
        kind: "NETWORK".into(),
        data: serde_json::to_value(&net).unwrap(),
      };
      let _ = metrics::create(item, pool).await;
    }
  }
}

pub fn spawn_metrics(pool: &Pool) {
  let pool = pool.clone();
  rt::Arbiter::new().exec_fn(move || {
    let client = MetrsdClient::connect("unix:///run/nanocl/metrics.sock");
    rt::spawn(async move {
      loop {
        match client.subscribe().await {
          Ok(mut stream) => {
            while let Some(res) = stream.next().await {
              match res {
                Ok(ev) => {
                  save_metric(&pool, ev).await;
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
