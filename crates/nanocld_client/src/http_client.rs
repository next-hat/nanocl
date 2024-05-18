use std::error::Error;

use ntex::{rt, http};
use nanocl_stubs::{generic::GenericListNspQuery, system::SslConfig};

use ntex::util::{Bytes, Stream};
use ntex::channel::mpsc::Receiver;
use futures::{StreamExt, TryStreamExt};

use nanocl_error::{
  io::{IoError, FromIo},
  http::HttpError,
  http_client::HttpClientError,
};

use crate::error::is_api_error;

pub const NANOCLD_DEFAULT_VERSION: &str = "0.15.0";

#[derive(Clone, Debug)]
pub struct ConnectOpts {
  /// Url to connect to
  pub url: String,
  /// Optional version
  pub version: Option<String>,
  /// Optional certificate path
  pub ssl: Option<SslConfig>,
}

#[derive(Clone)]
pub struct NanocldClient {
  pub url: String,
  pub version: String,
  pub unix_socket: Option<String>,
  pub ssl: Option<SslConfig>,
}

impl Default for ConnectOpts {
  fn default() -> Self {
    Self {
      url: String::from("unix:///run/nanocl/nanocl.sock"),
      version: None,
      ssl: None,
    }
  }
}

impl std::fmt::Display for NanocldClient {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.url)
  }
}

impl NanocldClient {
  pub fn connect_with_unix_default() -> Self {
    NanocldClient {
      unix_socket: Some(String::from("/run/nanocl/nanocl.sock")),
      version: format!("v{NANOCLD_DEFAULT_VERSION}"),
      url: "http://localhost".to_owned(),
      ssl: None,
    }
  }

  pub fn connect_to(opts: &ConnectOpts) -> Self {
    let url = opts.url.clone();
    let version = opts.version.clone();
    match url {
      url if url.starts_with("http://") || url.starts_with("https://") => {
        NanocldClient {
          url: url.to_owned(),
          unix_socket: None,
          version: version.unwrap_or(format!("v{NANOCLD_DEFAULT_VERSION}")),
          ssl: opts.ssl.clone(),
        }
      }
      url if url.starts_with("unix://") => {
        let path = url.trim_start_matches("unix://");
        NanocldClient {
          url: "http://localhost".to_owned(),
          unix_socket: Some(path.to_owned()),
          version: version.unwrap_or(format!("v{NANOCLD_DEFAULT_VERSION}")),
          ssl: None,
        }
      }
      _ => panic!("Invalid url: {}", url),
    }
  }

  pub fn set_version(&mut self, version: &str) {
    self.version = format!("v{version}")
  }

  pub fn connect_with_unix_version(version: &str) -> Self {
    NanocldClient {
      unix_socket: Some(String::from("/run/nanocl/nanocl.sock")),
      version: version.to_owned(),
      url: String::from("http://localhost"),
      ssl: None,
    }
  }

  fn gen_client(&self) -> http::client::Client {
    let mut client = http::client::Client::build();
    if let Some(unix_socket) = &self.unix_socket {
      let unix_socket = unix_socket.clone();
      client = client.connector(
        http::client::Connector::default()
          .connector(ntex::service::fn_service(move |_| {
            let unix_socket = unix_socket.clone();
            async { Ok::<_, _>(rt::unix_connect(unix_socket).await?) }
          }))
          .timeout(ntex::time::Millis::from_secs(100))
          .finish(),
      );
    }
    #[cfg(feature = "openssl")]
    {
      use openssl::ssl::{SslMethod, SslConnector, SslVerifyMode, SslFiletype};
      if let Some(ssl) = &self.ssl {
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_verify(SslVerifyMode::NONE);
        builder
          .set_certificate_file(&ssl.cert.clone().unwrap(), SslFiletype::PEM)
          .unwrap();
        builder
          .set_private_key_file(
            &ssl.cert_key.clone().unwrap(),
            SslFiletype::PEM,
          )
          .unwrap();
        client = ntex::http::client::Client::build().connector(
          http::client::Connector::default()
            .openssl(builder.build())
            .finish(),
        )
      }
    }
    client.timeout(ntex::time::Millis::from_secs(100)).finish()
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
    HttpClientError::IoError(*err.map_err_context(|| url.to_owned()))
  }

  fn gen_url(&self, url: &str) -> String {
    format!("{}/{}{}", self.url, self.version, url)
  }

  fn get(&self, url: &str) -> http::client::ClientRequest {
    self
      .gen_client()
      .get(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  fn delete(&self, url: &str) -> http::client::ClientRequest {
    self
      .gen_client()
      .delete(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  fn post(&self, url: &str) -> http::client::ClientRequest {
    self
      .gen_client()
      .post(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  fn patch(&self, url: &str) -> http::client::ClientRequest {
    self
      .gen_client()
      .patch(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  fn put(&self, url: &str) -> http::client::ClientRequest {
    self
      .gen_client()
      .put(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  fn head(&self, url: &str) -> http::client::ClientRequest {
    self
      .gen_client()
      .head(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  pub async fn send_get<Q>(
    &self,
    url: &str,
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

  pub fn convert_query<Q>(
    query: Option<&Q>,
  ) -> Result<GenericListNspQuery, HttpClientError>
  where
    Q: Clone + Default + TryInto<GenericListNspQuery>,
    Q::Error: ToString,
  {
    let query = query.cloned().unwrap_or_default();
    let query = query.try_into().map_err(|err| {
      HttpClientError::IoError(IoError::invalid_data(
        "Query".to_owned(),
        err.to_string(),
      ))
    })?;
    Ok(query)
  }

  pub async fn send_post<Q, B>(
    &self,
    url: &str,
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

  pub async fn send_post_stream<S, Q, E>(
    &self,
    url: &str,
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

  pub async fn send_delete<Q>(
    &self,
    url: &str,
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

  pub async fn send_patch<B, Q>(
    &self,
    url: &str,
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

  pub async fn send_head<Q>(
    &self,
    url: &str,
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

  pub async fn send_put<B, Q>(
    &self,
    url: &str,
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

  pub async fn res_json<R>(
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

  pub async fn res_stream<R>(
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
            let _ = tx.send(Err(HttpError::internal_server_error(format!(
              "Unable to read stream: {e}"
            ))));
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
            let _ = tx.send(Err(HttpError::internal_server_error(format!(
              "Unable to parse stream: {e}"
            ))));
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
