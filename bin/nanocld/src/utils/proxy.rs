use thiserror::Error;
use ntex::{
  rt,
  http::{
    Client, StatusCode,
    client::{
      Connector,
      error::{SendRequestError, JsonPayloadError},
      ClientResponse,
    },
  },
};

use nanocl_stubs::resource::ResourcePartial;

use crate::error::HttpError;

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
      .post(self.format_url("/rules"))
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
