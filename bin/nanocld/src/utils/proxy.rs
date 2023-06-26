use ntex::rt;
use futures::StreamExt;
use bollard_next::container::{LogsOptions, LogOutput};

use crate::repositories;
use crate::models::{DaemonState, HttpMetricPartial};

/// ## Spawn logger
///
/// Create a background thread that will watch the logs of the `ncproxy.system.c` container
/// The `ncproxy` is a container that run to update the proxy rules.
/// He will print http metrics to the logs.
/// This function will parse the logs and save the metrics to the database.
///
/// ## Arguments
///
/// - [state](DaemonState) - Daemon state
///
pub(crate) fn spawn_logger(state: &DaemonState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      let now = chrono::Utc::now().timestamp();
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
      while let Some(log) = res.next().await {
        match log {
          Err(e) => {
            log::warn!("Failed to get log: {}", e);
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
                      http_metric.to_db_model(&state.config.hostname);
                    let res = repositories::http_metric::create(
                      &http_metric,
                      &state.pool,
                    )
                    .await;
                    if let Err(e) = res {
                      log::warn!("Failed to save http metric: {}", e);
                    }
                  }
                  Err(e) => {
                    log::warn!("Failed to parse http metric: {}", e);
                  }
                }
              }
              log if log.starts_with("#STREAM") => {}
              _ => {}
            }
          }
        }
      }
    });
  });
}
