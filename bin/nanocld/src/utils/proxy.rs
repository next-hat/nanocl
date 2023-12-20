use ntex::rt;
use futures::StreamExt;

use bollard_next::container::{LogOutput, LogsOptions};

use crate::{
  repositories::generic::*,
  models::{
    ToMeticDb, DaemonState, HttpMetricPartial, StreamMetricPartial,
    HttpMetricDb, StreamMetricDb,
  },
};

/// Create a background thread that will watch the logs of the `ncproxy.system.c` container
/// The `ncproxy` is a container that run to update the proxy rules.
/// He will print http metrics to the logs.
/// This function will parse the logs and save the metrics to the database.
pub(crate) fn spawn_logger(state: &DaemonState) {
  let state = state.clone();
  log::trace!("proxy::spawn_logger: thread start");
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      let now = chrono::Utc::now().timestamp();
      log::info!("proxy::spawn_logger: subcribing");
      let mut res = state.docker_api.logs(
        "ncproxy.system.c",
        Some(LogsOptions::<String> {
          follow: true,
          stdout: true,
          stderr: true,
          since: now,
          ..Default::default()
        }),
      );
      log::info!("proxy::spawn_logger: subscribed");
      while let Some(log) = res.next().await {
        match log {
          Err(e) => {
            log::warn!("proxy::spawn_logger: {e}");
            continue;
          }
          Ok(log) => {
            let log = match &log {
              LogOutput::StdOut { message } => {
                String::from_utf8_lossy(message).to_string()
              }
              LogOutput::StdErr { message } => {
                String::from_utf8_lossy(message).to_string()
              }
              LogOutput::Console { message } => {
                String::from_utf8_lossy(message).to_string()
              }
              _ => continue,
            };
            match &log {
              log if log.starts_with("#HTTP") => {
                let log = log.trim_start_matches("#HTTP");
                let http_metric =
                  serde_json::from_str::<HttpMetricPartial>(log);
                match http_metric {
                  Ok(http_metric) => {
                    let http_metric =
                      http_metric.to_metric_db(&state.config.hostname);
                    let res =
                      HttpMetricDb::create_from(http_metric, &state.pool).await;
                    if let Err(err) = res {
                      log::warn!("proxy::spawn_logger: {err}");
                    }
                  }
                  Err(err) => {
                    log::warn!("proxy::spawn_logger: {err}");
                  }
                }
              }
              log if log.starts_with("#STREAM") => {
                let trimmed_log = log.trim_start_matches("#STREAM");
                let stream_metric =
                  serde_json::from_str::<StreamMetricPartial>(trimmed_log);
                match stream_metric
                  .map(|metric| metric.to_metric_db(&state.config.hostname))
                {
                  Ok(stream_db_model) => {
                    let insert_result =
                      StreamMetricDb::create_from(stream_db_model, &state.pool)
                        .await;
                    if let Err(err) = insert_result {
                      log::warn!("proxy::spawn_logger: {err}");
                    }
                  }
                  Err(err) => {
                    log::warn!("proxy::spawn_logger: {err}");
                  }
                }
              }
              _ => {}
            }
          }
        }
      }
    });
  });
}
