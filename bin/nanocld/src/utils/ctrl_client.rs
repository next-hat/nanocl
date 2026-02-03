use ntex::{
  ServiceFactory,
  client::{Client, ClientResponse},
  http::StatusCode,
  rt,
};

use nanocl_error::io::FromIo;
use nanocl_error::{http::HttpError, io::IoError};
use nanocl_error::{http_client::HttpClientError, io::IoResult};

/// Controller client
pub struct CtrlClient {
  /// Name of the controller eg: (ProxyRule)
  pub name: String,
  /// HTTP client
  pub client: Client,
  /// Base url
  pub base_url: String,
}

impl CtrlClient {
  /// Create a new controller client
  pub async fn new(name: &str, url: &str) -> IoResult<Self> {
    log::debug!("CtrlClient::new {name}: {url}");
    let (client, url) = match url {
      url if url.starts_with("unix://") => {
        let url = url.to_owned();
        let client = Client::builder()
          .connector::<&str>(
            ntex::client::Connector::default().connector(
              ntex::service::fn_service(move |_| {
                let unix_socket = url.trim_start_matches("unix://").to_owned();
                async {
                  Ok(
                    rt::unix_connect(unix_socket, ntex::SharedCfg::default())
                      .await?,
                  )
                }
              })
              .map_init_err(|_| unreachable!()),
            ),
          )
          .build(ntex::SharedCfg::default())
          .await
          .map_err(|err| {
            IoError::invalid_data(
              "Unable to build http client",
              err.to_string().as_str(),
            )
          })?;
        (client, "http://localhost")
      }
      url if url.starts_with("http://") || url.starts_with("https://") => {
        let client = Client::builder()
          .build(ntex::SharedCfg::default())
          .await
          .map_err(|err| {
            IoError::invalid_data(
              "Unable to build http client",
              err.to_string().as_str(),
            )
          })?;
        (client, url)
      }
      _ => panic!("Invalid url: {}", url),
    };
    Ok(Self {
      client,
      name: name.to_owned(),
      base_url: url.to_owned(),
    })
  }

  /// Format url with base url
  fn format_url(&self, path: &str) -> String {
    format!("{}{}", self.base_url, path)
  }

  /// Check if the response is an API error
  async fn is_api_error(
    &self,
    res: &ClientResponse,
    status: &StatusCode,
  ) -> Result<(), HttpClientError> {
    if status.is_server_error() || status.is_client_error() {
      let body = res
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.map_err_context(|| self.name.to_owned()))?;
      let msg = body["msg"]
        .as_str()
        .ok_or(HttpError::new(*status, String::default()))?;
      return Err(HttpClientError::HttpError(HttpError::new(
        *status,
        format!("{}: {msg}", self.name),
      )));
    }
    Ok(())
  }

  /// Parse http response to json
  async fn res_json<T>(
    &self,
    res: &ClientResponse,
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
  pub async fn apply_rule(
    &self,
    version: &str,
    name: &str,
    data: &serde_json::Value,
  ) -> Result<serde_json::Value, HttpClientError> {
    let url = self.format_url(&format!("/{version}/rules/{name}"));
    log::debug!("CtrlClient::apply_rule url: {}", url);
    let res = self
      .client
      .put(url)
      .send_json(data)
      .await
      .map_err(|err| err.map_err_context(|| self.name.to_owned()))?;
    let status = res.status();
    self.is_api_error(&res, &status).await?;
    self.res_json(&res).await
  }

  /// Call delete rule method on controller
  pub async fn delete_rule(
    &self,
    version: &str,
    name: &str,
  ) -> Result<(), HttpClientError> {
    let url = self.format_url(&format!("/{version}/rules/{name}"));
    log::debug!("CtrlClient::delete_rule url: {}", url);
    let res = self
      .client
      .delete(url)
      .send()
      .await
      .map_err(|err| err.map_err_context(|| self.name.to_owned()))?;
    let status = res.status();
    self.is_api_error(&res, &status).await?;
    Ok(())
  }
}
