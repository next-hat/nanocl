use std::error::Error;

use ntex::rt;
use ntex::http;
use ntex::util::{Bytes, Stream};
use ntex::channel::mpsc::Receiver;
use futures::{StreamExt, TryStreamExt};

use nanocl_utils::io_error::FromIo;
use nanocl_utils::http_error::HttpError;
use nanocl_utils::http_client_error::HttpClientError;

use crate::error::is_api_error;

const NANOCLD_DEFAULT_VERSION: &str = "0.6.0";

#[derive(Clone)]
pub struct NanocldClient {
  pub client: http::client::Client,
  pub url: String,
  pub version: String,
  pub unix_socket: Option<String>,
}

impl NanocldClient {
  pub fn connect_with_unix_default() -> Self {
    let client = http::client::Client::build()
      .connector(
        http::client::Connector::default()
          .connector(ntex::service::fn_service(|_| async {
            Ok::<_, _>(rt::unix_connect("/run/nanocl/nanocl.sock").await?)
          }))
          .timeout(ntex::time::Millis::from_secs(100))
          .finish(),
      )
      .timeout(ntex::time::Millis::from_secs(100))
      .finish();

    NanocldClient {
      client,
      unix_socket: Some(String::from("/run/nanocl/nanocl.sock")),
      version: format!("v{NANOCLD_DEFAULT_VERSION}"),
      url: String::from("http://localhost"),
    }
  }

  pub fn connect_to(url: &'static str) -> Self {
    let (client, url) = match url {
      url if url.starts_with("http://") || url.starts_with("https://") => {
        let client = http::client::Client::build()
          .connector(
            http::client::Connector::default()
              .timeout(ntex::time::Millis::from_secs(100))
              .finish(),
          )
          .timeout(ntex::time::Millis::from_secs(100))
          .finish();
        (client, url.to_owned())
      }
      url if url.starts_with("unix://") => {
        let client = http::client::Client::build()
          .connector(
            http::client::Connector::default()
              .connector(ntex::service::fn_service(move |_| async {
                let path = url.trim_start_matches("unix://");
                Ok::<_, _>(rt::unix_connect(path).await?)
              }))
              .timeout(ntex::time::Millis::from_secs(100))
              .finish(),
          )
          .timeout(ntex::time::Millis::from_secs(100))
          .finish();
        (client, "http://localhost".into())
      }
      _ => panic!("Invalid url: {}", url),
    };

    NanocldClient {
      url,
      client,
      unix_socket: None,
      version: format!("v{NANOCLD_DEFAULT_VERSION}"),
    }
  }

  pub fn set_version(&mut self, version: &str) {
    self.version = format!("v{version}")
  }

  pub fn connect_with_url(url: &str, version: &str) -> Self {
    let client = http::client::Client::build()
      .timeout(ntex::time::Millis::from_secs(100))
      .finish();

    NanocldClient {
      client,
      unix_socket: None,
      url: url.to_owned(),
      version: version.to_owned(),
    }
  }

  fn send_error(
    &self,
    err: http::client::error::SendRequestError,
  ) -> HttpClientError {
    let url = if let Some(url) = &self.unix_socket {
      url
    } else {
      &self.url
    };
    HttpClientError::IoError(*err.map_err_context(|| url.to_string()))
  }

  pub fn connect_with_unix_version(version: &str) -> Self {
    let client = http::client::Client::build()
      .connector(
        http::client::Connector::default()
          .connector(ntex::service::fn_service(|_| async {
            Ok::<_, _>(rt::unix_connect("/run/nanocl/nanocl.sock").await?)
          }))
          .timeout(ntex::time::Millis::from_secs(100))
          .finish(),
      )
      .timeout(ntex::time::Millis::from_secs(100))
      .finish();

    NanocldClient {
      client,
      unix_socket: Some(String::from("/run/nanocl/nanocl.sock")),
      version: version.to_owned(),
      url: String::from("http://localhost"),
    }
  }

  fn gen_url(&self, url: String) -> String {
    self.url.to_owned() + &url
  }

  fn get(&self, url: String) -> http::client::ClientRequest {
    self.client.get(self.gen_url(url))
  }

  fn delete(&self, url: String) -> http::client::ClientRequest {
    self
      .client
      .delete(self.gen_url(url))
      .header("User-Agent", "nanocld.client.rs")
  }

  fn post(&self, url: String) -> http::client::ClientRequest {
    self
      .client
      .post(self.gen_url(url))
      .header("User-Agent", "nanocld.client.rs")
  }

  fn patch(&self, url: String) -> http::client::ClientRequest {
    self
      .client
      .patch(self.gen_url(url))
      .header("User-Agent", "nanocld.client.rs")
  }

  fn put(&self, url: String) -> http::client::ClientRequest {
    self
      .client
      .put(self.gen_url(url))
      .header("User-Agent", "nanocld.client.rs")
  }

  fn head(&self, url: String) -> http::client::ClientRequest {
    self
      .client
      .head(self.gen_url(url))
      .header("User-Agent", "nanocld.client.rs")
  }

  pub(crate) async fn send_get<Q>(
    &self,
    url: String,
    query: Option<Q>,
  ) -> Result<http::client::ClientResponse, HttpClientError>
  where
    Q: serde::Serialize,
  {
    let mut req = self
      .get(url)
      .set_connection_type(http::ConnectionType::KeepAlive);
    if let Some(query) = query {
      req = req
        .query(&query)
        .map_err(|err| err.map_err_context(|| "Query"))?;
    }
    let mut res = req.send().await.map_err(|err| self.send_error(err))?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;
    Ok(res)
  }

  pub(crate) async fn send_post<Q, B>(
    &self,
    url: String,
    body: Option<B>,
    query: Option<Q>,
  ) -> Result<http::client::ClientResponse, HttpClientError>
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.post(url);
    if let Some(query) = query {
      req = req
        .query(&query)
        .map_err(|err| err.map_err_context(|| "Query"))?;
    }
    let mut res = match body {
      None => req.send().await.map_err(|err| self.send_error(err))?,
      Some(body) => req
        .send_json(&body)
        .await
        .map_err(|err| self.send_error(err))?,
    };

    let status = res.status();
    is_api_error(&mut res, &status).await?;
    Ok(res)
  }

  pub(crate) async fn send_post_stream<S, Q, E>(
    &self,
    url: String,
    stream: S,
    query: Option<Q>,
  ) -> Result<http::client::ClientResponse, HttpClientError>
  where
    S: Stream<Item = Result<Bytes, E>> + Unpin + 'static,
    Q: serde::Serialize,
    E: Error + 'static,
  {
    let mut req = self.post(url);
    if let Some(query) = query {
      req = req
        .query(&query)
        .map_err(|err| err.map_err_context(|| "Query"))?;
    }
    let mut res = req
      .send_stream(stream)
      .await
      .map_err(|err| self.send_error(err))?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(res)
  }

  pub(crate) async fn send_delete<Q>(
    &self,
    url: String,
    query: Option<Q>,
  ) -> Result<http::client::ClientResponse, HttpClientError>
  where
    Q: serde::Serialize,
  {
    let mut req = self.delete(url);
    if let Some(query) = query {
      req = req
        .query(&query)
        .map_err(|err| err.map_err_context(|| "Query"))?;
    }
    let mut res = req.send().await.map_err(|err| self.send_error(err))?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(res)
  }

  pub(crate) async fn send_patch<B, Q>(
    &self,
    url: String,
    body: Option<B>,
    query: Option<Q>,
  ) -> Result<http::client::ClientResponse, HttpClientError>
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.patch(url);
    if let Some(query) = query {
      req = req
        .query(&query)
        .map_err(|err| err.map_err_context(|| "Query"))?;
    }
    let mut res = match body {
      None => req.send().await.map_err(|err| self.send_error(err))?,
      Some(body) => req
        .send_json(&body)
        .await
        .map_err(|err| self.send_error(err))?,
    };

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(res)
  }

  pub(crate) async fn send_head<Q>(
    &self,
    url: String,
    query: Option<Q>,
  ) -> Result<http::client::ClientResponse, HttpClientError>
  where
    Q: serde::Serialize,
  {
    let mut req = self.head(url);
    if let Some(query) = query {
      req = req
        .query(&query)
        .map_err(|err| err.map_err_context(|| "Query"))?;
    }

    let mut res = req.send().await.map_err(|err| self.send_error(err))?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(res)
  }

  pub(crate) async fn send_put<B, Q>(
    &self,
    url: String,
    body: Option<B>,
    query: Option<Q>,
  ) -> Result<http::client::ClientResponse, HttpClientError>
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.put(url);
    if let Some(query) = query {
      req = req
        .query(&query)
        .map_err(|err| err.map_err_context(|| "Query"))?;
    }
    let mut res = match body {
      None => req.send().await.map_err(|err| self.send_error(err))?,
      Some(body) => req
        .send_json(&body)
        .await
        .map_err(|err| self.send_error(err))?,
    };

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(res)
  }

  pub(crate) async fn res_json<R>(
    mut res: http::client::ClientResponse,
  ) -> Result<R, HttpClientError>
  where
    R: serde::de::DeserializeOwned + Send + 'static,
  {
    let body = res
      .json::<R>()
      .limit(20_000_000)
      .await
      .map_err(|err| err.map_err_context(|| "Payload limit 20_000_000"))?;
    Ok(body)
  }

  pub(crate) async fn res_stream<R>(
    res: http::client::ClientResponse,
  ) -> Receiver<Result<R, HttpError>>
  where
    R: serde::de::DeserializeOwned + Send + 'static,
  {
    let mut stream = res.into_stream();
    let (tx, rx) = ntex::channel::mpsc::channel();
    rt::spawn(async move {
      let mut payload: Vec<u8> = Vec::new();
      while let Some(item) = stream.next().await {
        let bytes = match item {
          Ok(bytes) => bytes,
          Err(e) => {
            let _ = tx.send(Err(HttpError {
              status: http::StatusCode::INTERNAL_SERVER_ERROR,
              msg: format!("Unable to read stream got error : {e}"),
            }));
            break;
          }
        };
        payload.extend(bytes.to_vec());
        if bytes.last() != Some(&b'\n') {
          continue;
        }
        let t = match serde_json::from_slice::<R>(&payload) {
          Ok(t) => t,
          Err(e) => {
            let _ = tx.send(Err(HttpError {
              status: http::StatusCode::INTERNAL_SERVER_ERROR,
              msg: format!("Unable to parse stream got error : {e}"),
            }));
            break;
          }
        };
        payload.clear();
        if tx.send(Ok(t)).is_err() {
          break;
        }
      }
      tx.close();
    });
    rx
  }
}
