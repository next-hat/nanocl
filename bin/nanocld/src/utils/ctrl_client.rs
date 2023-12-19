use ntex::rt;
use ntex::http::{Client, StatusCode};
use ntex::http::client::{Connector, ClientResponse};

use nanocl_error::io::FromIo;
use nanocl_error::http::HttpError;
use nanocl_error::http_client::HttpClientError;

/// Controller client
pub(crate) struct CtrlClient {
  /// Name of the controller eg: (ProxyRule)
  pub(crate) name: String,
  /// HTTP client
  pub(crate) client: Client,
  /// Base url
  pub(crate) base_url: String,
}

impl CtrlClient {
  /// Create a new controller client
  pub(crate) fn new(name: &str, url: &str) -> Self {
    log::debug!("CtrlClient::new {name}: {url}");
    let (client, url) = match url {
      url if url.starts_with("unix://") => {
        let url = url.to_owned();
        let client = Client::build()
          .connector(
            Connector::default()
              .connector(ntex::service::fn_service(move |_| {
                let path = url.trim_start_matches("unix://").to_owned();
                async move { Ok(rt::unix_connect(path).await?) }
              }))
              .timeout(ntex::time::Millis::from_secs(50))
              .finish(),
          )
          .timeout(ntex::time::Millis::from_secs(50))
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
      name: name.to_owned(),
      base_url: url.to_owned(),
    }
  }

  /// Format url with base url
  fn format_url(&self, path: &str) -> String {
    format!("{}{}", self.base_url, path)
  }

  /// Check if the response is an API error
  async fn is_api_error(
    &self,
    res: &mut ClientResponse,
    status: &StatusCode,
  ) -> Result<(), HttpClientError> {
    if status.is_server_error() || status.is_client_error() {
      let body = res
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.map_err_context(|| self.name.to_owned()))?;
      let msg = body["msg"].as_str().ok_or(HttpError {
        status: *status,
        msg: String::default(),
      })?;
      return Err(HttpClientError::HttpError(HttpError {
        status: *status,
        msg: format!("{}: {msg}", self.name),
      }));
    }
    Ok(())
  }

  /// Parse http response to json
  async fn res_json<T>(
    &self,
    res: &mut ClientResponse,
  ) -> Result<T, HttpClientError>
  where
    T: serde::de::DeserializeOwned,
  {
    let body = res
      .json::<T>()
      .await
      .map_err(|err| err.map_err_context(|| self.name.to_owned()))?;
    Ok(body)
  }

  /// Call apply rule method on controller
  pub(crate) async fn apply_rule(
    &self,
    version: &str,
    name: &str,
    data: &serde_json::Value,
  ) -> Result<serde_json::Value, HttpClientError> {
    let url = self.format_url(&format!("/{version}/rules/{name}"));
    log::debug!("CtrlClient::apply_rule url: {}", url);
    let mut res = self
      .client
      .put(url)
      .send_json(data)
      .await
      .map_err(|err| err.map_err_context(|| self.name.to_owned()))?;
    let status = res.status();
    self.is_api_error(&mut res, &status).await?;
    self.res_json(&mut res).await
  }

  /// Call delete rule method on controller
  pub(crate) async fn delete_rule(
    &self,
    version: &str,
    name: &str,
  ) -> Result<(), HttpClientError> {
    let url = self.format_url(&format!("/{version}/rules/{name}"));
    log::debug!("CtrlClient::delete_rule url: {}", url);
    let mut res = self
      .client
      .delete(url)
      .send()
      .await
      .map_err(|err| err.map_err_context(|| self.name.to_owned()))?;
    let status = res.status();
    self.is_api_error(&mut res, &status).await?;
    Ok(())
  }
}
