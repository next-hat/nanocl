use std::error::Error;

use ntex::rt;
use ntex::util::{Bytes, Stream};
use ntex::channel::mpsc::Receiver;
use ntex::http::{Client, StatusCode};
use ntex::http::client::{Connector, ClientRequest, ClientResponse};
use futures::{StreamExt, TryStreamExt};

use ntex::http::client::error::SendRequestError as NtexSendRequestError;
use crate::error::{ApiError, NanocldClientError, is_api_error, SendRequestError};

const NANOCLD_DEFAULT_VERSION: &str = "0.3.0";

#[derive(Clone)]
pub struct NanocldClient {
  pub client: Client,
  pub url: String,
  pub version: String,
  pub unix_socket: Option<String>,
}

impl NanocldClient {
  pub fn connect_with_unix_default() -> Self {
    let client = Client::build()
      .connector(
        Connector::default()
          .connector(ntex::service::fn_service(|_| async {
            Ok::<_, _>(rt::unix_connect("/run/nanocl/nanocl.sock").await?)
          }))
          .timeout(ntex::time::Millis::from_secs(50))
          .finish(),
      )
      .timeout(ntex::time::Millis::from_secs(50))
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
        let client = Client::build()
          .connector(
            Connector::default()
              .timeout(ntex::time::Millis::from_secs(20))
              .finish(),
          )
          .timeout(ntex::time::Millis::from_secs(20))
          .finish();
        (client, url.to_owned())
      }
      url if url.starts_with("unix://") => {
        let client = Client::build()
          .connector(
            Connector::default()
              .connector(ntex::service::fn_service(move |_| async {
                let path = url.trim_start_matches("unix://");
                Ok::<_, _>(rt::unix_connect(path).await?)
              }))
              .timeout(ntex::time::Millis::from_secs(50))
              .finish(),
          )
          .timeout(ntex::time::Millis::from_secs(50))
          .finish();
        (client, "http://localhost".into())
      }
      _ => panic!("Invalid url: {}", url),
    };

    NanocldClient {
      client,
      unix_socket: None,
      url,
      version: format!("v{NANOCLD_DEFAULT_VERSION}"),
    }
  }

  pub fn connect_with_url(url: &str, version: &str) -> Self {
    let client = Client::build()
      .timeout(ntex::time::Millis::from_secs(50))
      .finish();

    NanocldClient {
      client,
      unix_socket: None,
      url: url.to_owned(),
      version: version.to_owned(),
    }
  }

  fn send_error(&self, err: NtexSendRequestError) -> SendRequestError {
    SendRequestError {
      msg: format!(
        "Cannot send request to {}: {err}",
        if let Some(url) = &self.unix_socket {
          url
        } else {
          &self.url
        }
      ),
    }
  }

  pub fn connect_with_unix_version(version: &str) -> Self {
    let client = Client::build()
      .connector(
        Connector::default()
          .connector(ntex::service::fn_service(|_| async {
            Ok::<_, _>(rt::unix_connect("/run/nanocl/nanocl.sock").await?)
          }))
          .timeout(ntex::time::Millis::from_secs(50))
          .finish(),
      )
      .timeout(ntex::time::Millis::from_secs(50))
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

  fn get(&self, url: String) -> ClientRequest {
    self.client.get(self.gen_url(url))
  }

  fn delete(&self, url: String) -> ClientRequest {
    self.client.delete(self.gen_url(url))
  }

  fn post(&self, url: String) -> ClientRequest {
    self.client.post(self.gen_url(url))
  }

  fn patch(&self, url: String) -> ClientRequest {
    self.client.patch(self.gen_url(url))
  }

  fn put(&self, url: String) -> ClientRequest {
    self.client.put(self.gen_url(url))
  }

  pub(crate) async fn send_get<Q>(
    &self,
    url: String,
    query: Option<Q>,
  ) -> Result<ClientResponse, NanocldClientError>
  where
    Q: serde::Serialize,
  {
    let mut req = self.get(url);
    if let Some(query) = query {
      req = req.query(&query)?;
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
  ) -> Result<ClientResponse, NanocldClientError>
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.post(url);
    if let Some(query) = query {
      req = req.query(&query)?;
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
  ) -> Result<ClientResponse, NanocldClientError>
  where
    S: Stream<Item = Result<Bytes, E>> + Unpin + 'static,
    Q: serde::Serialize,
    E: Error + 'static,
  {
    let mut req = self.post(url);
    if let Some(query) = query {
      req = req.query(&query)?;
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
  ) -> Result<ClientResponse, NanocldClientError>
  where
    Q: serde::Serialize,
  {
    let mut req = self.delete(url);
    if let Some(query) = query {
      req = req.query(&query)?;
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
  ) -> Result<ClientResponse, NanocldClientError>
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.patch(url);
    if let Some(query) = query {
      req = req.query(&query)?;
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

  pub(crate) async fn send_put<B, Q>(
    &self,
    url: String,
    body: Option<B>,
    query: Option<Q>,
  ) -> Result<ClientResponse, NanocldClientError>
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.put(url);
    if let Some(query) = query {
      req = req.query(&query)?;
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
    mut res: ClientResponse,
  ) -> Result<R, NanocldClientError>
  where
    R: serde::de::DeserializeOwned + Send + 'static,
  {
    let body = res.json::<R>().limit(20_000_000).await?;
    Ok(body)
  }

  pub(crate) async fn res_stream<R>(
    res: ClientResponse,
  ) -> Receiver<Result<R, ApiError>>
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
            let _ = tx.send(Err(ApiError {
              status: StatusCode::INTERNAL_SERVER_ERROR,
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
            let _ = tx.send(Err(ApiError {
              status: StatusCode::INTERNAL_SERVER_ERROR,
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
