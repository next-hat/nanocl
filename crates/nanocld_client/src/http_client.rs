use std::error::Error;

use futures::{StreamExt, TryStreamExt};
use ntex::{
  channel::mpsc::Receiver,
  rt,
  time::Millis,
  util::{Bytes, Stream},
};

use nanocl_error::io::IoResult;
use nanocl_error::{
  http::HttpError,
  http_client::HttpClientError,
  io::{FromIo, IoError},
};
use nanocl_stubs::{generic::GenericListQueryNsp, system::SslConfig};

use crate::error::is_api_error;

pub const NANOCLD_DEFAULT_VERSION: &str = "0.18.0";

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

  pub fn connect_to(opts: &ConnectOpts) -> IoResult<Self> {
    let url = opts.url.clone();
    let version = opts.version.clone();
    match url {
      url if url.starts_with("http://") || url.starts_with("https://") => {
        Ok(NanocldClient {
          url: url.to_owned(),
          ssl: opts.ssl.clone(),
          unix_socket: None,
          version: version.unwrap_or(format!("v{NANOCLD_DEFAULT_VERSION}")),
        })
      }
      url if url.starts_with("unix://") => {
        let path = url.trim_start_matches("unix://");
        Ok(NanocldClient {
          ssl: None,
          url: "http://localhost".to_owned(),
          unix_socket: Some(path.to_owned()),
          version: version.unwrap_or(format!("v{NANOCLD_DEFAULT_VERSION}")),
        })
      }
      _ => Err(IoError::invalid_data("Invalid url", &url)),
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

  async fn gen_client(&self) -> IoResult<ntex::client::Client> {
    #[allow(unused_mut)]
    let mut client = ntex::client::Client::builder()
      .response_payload_timeout(Millis::from_secs(20))
      .response_timeout(Millis::from_secs(20));
    #[cfg(not(target_os = "windows"))]
    {
      if let Some(unix_socket) = &self.unix_socket {
        use ntex::ServiceFactory;
        let unix_socket = unix_socket.clone();
        client = client.connector::<&str>(
          ntex::client::Connector::default().connector(
            ntex::service::fn_service(move |_| {
              let unix_socket = unix_socket.clone();
              async {
                Ok(
                  rt::unix_connect(unix_socket, ntex::SharedCfg::default())
                    .await?,
                )
              }
            })
            .map_init_err(|_| unreachable!()),
          ),
        );
      }
    }
    #[cfg(feature = "openssl")]
    {
      use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
      if let Some(ssl) = &self.ssl {
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        if ssl.verify {
          builder.set_verify(
            SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT,
          );
          let cert_ca = openssl::x509::X509::from_pem(
            ssl.cert_ca.clone().expect("Ssl.ca to be fill").as_bytes(),
          )
          .expect("Invalid ssl cert ca");
          // Create an X509Store and add the certificate
          let mut store_builder = openssl::x509::store::X509StoreBuilder::new()
            .expect("Failed to create X509 store builder");
          store_builder
            .add_cert(cert_ca)
            .expect("Failed to add CA certificate to store");
          let store = store_builder.build();
          builder.set_cert_store(store);
        } else {
          builder.set_verify(SslVerifyMode::NONE);
        }
        let cert = openssl::x509::X509::from_pem(
          ssl.cert.clone().expect("Ssl.cert to be fill").as_bytes(),
        )
        .map_err(|err| {
          IoError::invalid_data("Invalid ssl cert", err.to_string().as_str())
        })?;
        let cert_key = openssl::pkey::PKey::private_key_from_pem(
          ssl
            .cert_key
            .clone()
            .expect("Ssl.cert_key to be fill")
            .as_bytes(),
        )
        .map_err(|err| {
          IoError::invalid_data(
            "Invalid ssl cert key",
            err.to_string().as_str(),
          )
        })?;
        builder.set_certificate(&cert).map_err(|err| {
          IoError::invalid_data("Invalid ssl cert", err.to_string().as_str())
        })?;
        builder.set_private_key(&cert_key).map_err(|err| {
          IoError::invalid_data(
            "Invalid ssl cert key",
            err.to_string().as_str(),
          )
        })?;
        client = ntex::client::Client::builder().connector::<&str>(
          ntex::client::Connector::default().openssl(builder.build()),
        )
      }
    }

    client
      .build(ntex::SharedCfg::default())
      .await
      .map_err(|err| {
        IoError::invalid_data(
          "Unable to build http client",
          err.to_string().as_str(),
        )
      })
  }

  fn send_error(
    &self,
    err: ntex::client::error::ClientError,
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

  async fn get(&self, url: &str) -> IoResult<ntex::client::ClientRequest> {
    Ok(
      self
        .gen_client()
        .await?
        .get(self.gen_url(url))
        .header("User-Agent", "nanocld_client"),
    )
  }

  async fn delete(&self, url: &str) -> IoResult<ntex::client::ClientRequest> {
    Ok(
      self
        .gen_client()
        .await?
        .delete(self.gen_url(url))
        .header("User-Agent", "nanocld_client"),
    )
  }

  async fn post(&self, url: &str) -> IoResult<ntex::client::ClientRequest> {
    Ok(
      self
        .gen_client()
        .await?
        .post(self.gen_url(url))
        .header("User-Agent", "nanocld_client"),
    )
  }

  async fn patch(&self, url: &str) -> IoResult<ntex::client::ClientRequest> {
    Ok(
      self
        .gen_client()
        .await?
        .patch(self.gen_url(url))
        .header("User-Agent", "nanocld_client"),
    )
  }

  async fn put(&self, url: &str) -> IoResult<ntex::client::ClientRequest> {
    Ok(
      self
        .gen_client()
        .await?
        .put(self.gen_url(url))
        .header("User-Agent", "nanocld_client"),
    )
  }

  async fn head(&self, url: &str) -> IoResult<ntex::client::ClientRequest> {
    Ok(
      self
        .gen_client()
        .await?
        .head(self.gen_url(url))
        .header("User-Agent", "nanocld_client"),
    )
  }

  pub async fn send_get<Q>(
    &self,
    url: &str,
    query: Option<Q>,
  ) -> Result<ntex::client::ClientResponse, HttpClientError>
  where
    Q: serde::Serialize,
  {
    let mut req = self.get(url).await?;
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
  ) -> Result<GenericListQueryNsp, HttpClientError>
  where
    Q: Clone + Default + TryInto<GenericListQueryNsp>,
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
  ) -> Result<ntex::client::ClientResponse, HttpClientError>
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.post(url).await?;
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
  ) -> Result<ntex::client::ClientResponse, HttpClientError>
  where
    S: Stream<Item = Result<Bytes, E>> + Unpin + 'static,
    Q: serde::Serialize,
    E: Error + 'static,
  {
    let mut req = self.post(url).await?;
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
  ) -> Result<ntex::client::ClientResponse, HttpClientError>
  where
    Q: serde::Serialize,
  {
    let mut req = self.delete(url).await?;
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
  ) -> Result<ntex::client::ClientResponse, HttpClientError>
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.patch(url).await?;
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
  ) -> Result<ntex::client::ClientResponse, HttpClientError>
  where
    Q: serde::Serialize,
  {
    let mut req = self.head(url).await?;
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
  ) -> Result<ntex::client::ClientResponse, HttpClientError>
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.put(url).await?;
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
    res: ntex::client::ClientResponse,
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
    res: ntex::client::ClientResponse,
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
