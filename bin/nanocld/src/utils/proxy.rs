use ntex::rt;
use ntex::http::{Client, StatusCode};
use ntex::http::client::{
  Connector, ClientResponse,
  error::{SendRequestError, JsonPayloadError},
};
use thiserror::Error;
use futures::StreamExt;
use bollard_next::container::{LogsOptions, LogOutput};

use nanocl_stubs::resource::ResourcePartial;

use crate::repositories;
use crate::error::HttpError;
use crate::models::{DaemonState, HttpMetricPartial};

pub struct ProxyClient {
  pub(crate) client: Client,
  pub(crate) url: String,
}

#[derive(Debug, Error)]
pub enum ProxyClientError {
  #[error("Failed to send request: {0}")]
  SendRequest(#[from] SendRequestError),
  #[error("Failed to parse json: {0}")]
  JsonPayload(#[from] JsonPayloadError),
  #[error(transparent)]
  HttpResponse(#[from] HttpError),
}

impl ProxyClient {
  pub(crate) fn new(url: &'static str) -> Self {
    let (client, url) = match url {
      url if url.starts_with("unix://") => {
        let client = Client::build()
          .connector(
            Connector::default()
              .connector(ntex::service::fn_service(move |_| async {
                let path = url.trim_start_matches("unix://");
                Ok::<_, _>(rt::unix_connect(path).await?)
              }))
              .timeout(ntex::time::Millis::from_secs(20))
              .finish(),
          )
          .finish();

        (client, "http://localhost")
      }
      url if url.starts_with("http://") || url.starts_with("https://") => {
        let client = Client::build().finish();
        (client, url)
      }
      _ => panic!("Invalid url: {}", url),
    };

    Self {
      client,
      url: url.to_owned(),
    }
  }

  pub(crate) fn unix_default() -> Self {
    Self::new("unix:///run/nanocl/proxy.sock")
  }

  fn format_url(&self, path: &str) -> String {
    format!("{}{}", self.url, path)
  }

  async fn is_api_error(
    res: &mut ClientResponse,
    status: &StatusCode,
  ) -> Result<(), ProxyClientError> {
    if status.is_server_error() || status.is_client_error() {
      let body = res.json::<serde_json::Value>().await?;
      let msg = body["msg"].as_str().ok_or(HttpError {
        status: *status,
        msg: String::default(),
      })?;
      return Err(ProxyClientError::HttpResponse(HttpError {
        status: *status,
        msg: msg.to_owned(),
      }));
    }
    Ok(())
  }

  async fn res_json<T>(res: &mut ClientResponse) -> Result<T, ProxyClientError>
  where
    T: serde::de::DeserializeOwned,
  {
    let body = res.json::<T>().await?;
    Ok(body)
  }

  pub(crate) async fn apply_rule(
    &self,
    resource: &ResourcePartial,
  ) -> Result<ResourcePartial, ProxyClientError> {
    let mut res = self
      .client
      .put(self.format_url("/rules"))
      .send_json(resource)
      .await?;
    let status = res.status();
    Self::is_api_error(&mut res, &status).await?;

    Self::res_json(&mut res).await
  }

  pub(crate) async fn delete_rule(
    &self,
    name: &str,
    kind: &str,
  ) -> Result<(), ProxyClientError> {
    let mut res = self
      .client
      .delete(self.format_url(&format!("/rules/{}/{}", kind, name)))
      .send()
      .await?;
    let status = res.status();
    Self::is_api_error(&mut res, &status).await?;

    Ok(())
  }
}

pub(crate) fn spawn_logger(state: &DaemonState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      let now = chrono::Utc::now().timestamp();
      let mut res = state.docker_api.logs(
        "ncdproxy.system.c",
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
                    println!("{:?}", http_metric);
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
