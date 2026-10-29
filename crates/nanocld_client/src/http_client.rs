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

pub const NANOCLD_DEFAULT_VERSION: &str = "0.19.0";

#[cfg(feature = "openssl")]
pub(crate) fn tls_connector(
  ssl: Option<&SslConfig>,
) -> IoResult<openssl::ssl::SslConnector> {
  use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
  let mut builder = SslConnector::builder(SslMethod::tls()).map_err(|err| {
    IoError::invalid_data("Unable to configure TLS", &err.to_string())
  })?;
  let Some(ssl) = ssl else {
    // SslConnector loads the system trust store and verifies the hostname.
    // ntex's default connector disables verification, so both HTTP and console
    // clients must use this helper even without a custom client identity.
    builder.set_verify(SslVerifyMode::PEER);
    return Ok(builder.build());
  };
  if ssl.verify {
    builder
      .set_verify(SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT);
    let cert_ca = openssl::x509::X509::from_pem(
      ssl.cert_ca.clone().expect("Ssl.ca to be fill").as_bytes(),
    )
    .expect("Invalid ssl cert ca");
    let mut store_builder = openssl::x509::store::X509StoreBuilder::new()
      .expect("Failed to create X509 store builder");
    store_builder
      .add_cert(cert_ca)
      .expect("Failed to add CA certificate to store");
    builder.set_cert_store(store_builder.build());
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
    IoError::invalid_data("Invalid ssl cert key", err.to_string().as_str())
  })?;
  builder.set_certificate(&cert).map_err(|err| {
    IoError::invalid_data("Invalid ssl cert", err.to_string().as_str())
  })?;
  builder.set_private_key(&cert_key).map_err(|err| {
    IoError::invalid_data("Invalid ssl cert key", err.to_string().as_str())
  })?;
  Ok(builder.build())
}

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
    let cfg = ntex::SharedCfg::new("nanocld-client").add(
      ntex::client::ClientConfig::new()
        .set_response_payload_timeout(Millis::from_secs(20))
        .set_response_timeout(Millis::from_secs(20)),
    );
    let client = ntex::client::Client::builder();
    #[cfg(not(target_os = "windows"))]
    let client = if let Some(unix_socket) = &self.unix_socket {
      let unix_socket = unix_socket.clone();
      client.connector(async move |cfg: &ntex::SharedCfg, _| {
        Ok(
          rt::unix_connect(&unix_socket, cfg.clone())
            .await
            .map_err(ntex::connect::ConnectError::from)?,
        )
      })
    } else {
      client
    };
    #[cfg(feature = "openssl")]
    let client = if self.ssl.is_some() || self.url.starts_with("https://") {
      client.openssl(tls_connector(self.ssl.as_ref())?)
    } else {
      client
    };

    Ok(client.build(cfg))
  }

  pub(crate) fn send_error(
    &self,
    err: ntex::error::Error<ntex::client::error::ClientError>,
  ) -> HttpClientError {
    let url = if let Some(url) = &self.unix_socket {
      url
    } else {
      &self.url
    };
    HttpClientError::IoError(*err.map_err_context(|| url.to_owned()))
  }

  pub(crate) fn gen_url(&self, url: &str) -> String {
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

  pub(crate) async fn post(
    &self,
    url: &str,
  ) -> IoResult<ntex::client::ClientRequest> {
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
      req = req.query(&query);
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
      req = req.query(&query);
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
      req = req.query(&query);
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
      req = req.query(&query);
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
      req = req.query(&query);
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
      req = req.query(&query);
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
      req = req.query(&query);
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
      'stream: while let Some(item) = std::future::poll_fn(|cx| {
        // A quiet HTTP stream may never yield another event. Register receiver
        // cancellation independently so dropping a waiter releases its reader.
        if tx.poll_closed(cx).is_ready() {
          return std::task::Poll::Ready(None);
        }
        stream.poll_next_unpin(cx)
      })
      .await
      {
        let bytes = match item {
          Ok(bytes) => bytes,
          Err(e) => {
            let _ = tx.send(Err(HttpError::internal_server_error(format!(
              "Unable to read stream: {e}"
            ))));
            break;
          }
        };
        let buffered_len = payload.len();
        payload.extend_from_slice(&bytes);
        let mut start = 0;
        // Retained bytes have already been scanned for record boundaries.
        for (offset, byte) in bytes.iter().enumerate() {
          if *byte != b'\n' {
            continue;
          }
          let end = buffered_len + offset;
          let t = match serde_json::from_slice::<R>(&payload[start..=end]) {
            Ok(t) => t,
            Err(e) => {
              let _ = tx.send(Err(HttpError::internal_server_error(format!(
                "Unable to parse stream: {e}"
              ))));
              break 'stream;
            }
          };
          start = end + 1;
          if tx.send(Ok(t)).is_err() {
            break 'stream;
          }
        }
        payload.drain(..start);
      }
      tx.close();
    });
    rx
  }
}

#[cfg(test)]
mod tests {
  use ntex::{client::TestResponse, http::Payload};
  use serde_json::{Value, json};

  use super::*;

  #[cfg(feature = "openssl")]
  #[test]
  fn tls_connector_without_config_verifies_server_without_client_identity() {
    let connector = tls_connector(None).unwrap();
    assert_eq!(
      connector.context().verify_mode(),
      openssl::ssl::SslVerifyMode::PEER
    );
    assert!(connector.context().certificate().is_none());
    assert!(connector.context().private_key().is_none());
  }

  #[cfg(feature = "openssl")]
  fn tls_test_identity() -> (
    openssl::x509::X509,
    openssl::pkey::PKey<openssl::pkey::Private>,
  ) {
    use openssl::{asn1::Asn1Time, ec::EcGroup, ec::EcKey, nid::Nid};
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
    let key =
      openssl::pkey::PKey::from_ec_key(EcKey::generate(&group).unwrap())
        .unwrap();
    let mut name = openssl::x509::X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", "server.test").unwrap();
    let name = name.build();
    let mut cert = openssl::x509::X509::builder().unwrap();
    cert.set_version(2).unwrap();
    let serial = openssl::bn::BigNum::from_u32(1)
      .unwrap()
      .to_asn1_integer()
      .unwrap();
    cert.set_serial_number(&serial).unwrap();
    cert.set_subject_name(&name).unwrap();
    cert.set_issuer_name(&name).unwrap();
    cert.set_pubkey(&key).unwrap();
    cert
      .set_not_before(&Asn1Time::days_from_now(0).unwrap())
      .unwrap();
    cert
      .set_not_after(&Asn1Time::days_from_now(1).unwrap())
      .unwrap();
    let subject_alt_name =
      openssl::x509::extension::SubjectAlternativeName::new()
        .dns("server.test")
        .build(&cert.x509v3_context(None, None))
        .unwrap();
    cert.append_extension(subject_alt_name).unwrap();
    cert
      .sign(&key, openssl::hash::MessageDigest::sha256())
      .unwrap();
    (cert.build(), key)
  }

  #[cfg(feature = "openssl")]
  fn tls_test_config(
    cert: &openssl::x509::X509,
    key: &openssl::pkey::PKey<openssl::pkey::Private>,
  ) -> SslConfig {
    let cert = String::from_utf8(cert.to_pem().unwrap()).unwrap();
    SslConfig {
      cert_ca: Some(cert.clone()),
      cert: Some(cert),
      cert_key: Some(
        String::from_utf8(key.private_key_to_pem_pkcs8().unwrap()).unwrap(),
      ),
      ..Default::default()
    }
  }

  #[cfg(feature = "openssl")]
  #[test]
  fn tls_connector_preserves_configured_verify_and_client_identity() {
    let (cert, key) = tls_test_identity();
    let mut ssl = tls_test_config(&cert, &key);
    for verify in [false, true] {
      ssl.verify = verify;
      let connector = tls_connector(Some(&ssl)).unwrap();
      let context = connector.context();
      assert_eq!(
        context.verify_mode(),
        if verify {
          openssl::ssl::SslVerifyMode::PEER
            | openssl::ssl::SslVerifyMode::FAIL_IF_NO_PEER_CERT
        } else {
          openssl::ssl::SslVerifyMode::NONE
        }
      );
      assert!(
        context
          .certificate()
          .unwrap()
          .public_key()
          .unwrap()
          .public_eq(&key)
      );
      assert!(context.private_key().unwrap().public_eq(&key));
    }
  }

  #[cfg(feature = "openssl")]
  #[test]
  fn tls_connector_rejects_untrusted_servers_and_wrong_hostnames() {
    use openssl::ssl::{ErrorCode, Ssl, SslContext, SslMethod, SslStream};
    use std::{cell::RefCell, collections::VecDeque, io, rc::Rc};

    // Drive real TLS handshakes over paired memory queues, without listeners,
    // threads, external certificates, or timing-sensitive waits.
    struct Pipe {
      incoming: Rc<RefCell<VecDeque<u8>>>,
      outgoing: Rc<RefCell<VecDeque<u8>>>,
    }
    impl io::Read for Pipe {
      fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
          return Ok(0);
        }
        let mut incoming = self.incoming.borrow_mut();
        if incoming.is_empty() {
          return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = buffer.len().min(incoming.len());
        for byte in &mut buffer[..count] {
          *byte = incoming.pop_front().unwrap();
        }
        Ok(count)
      }
    }
    impl io::Write for Pipe {
      fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.outgoing.borrow_mut().extend(bytes);
        Ok(bytes.len())
      }
      fn flush(&mut self) -> io::Result<()> {
        Ok(())
      }
    }

    let (cert, key) = tls_test_identity();
    let mut server_context =
      SslContext::builder(SslMethod::tls_server()).unwrap();
    server_context.set_certificate(&cert).unwrap();
    server_context.set_private_key(&key).unwrap();
    let server_context = server_context.build();
    let handshake = |connector: openssl::ssl::SslConnector, hostname: &str| {
      let to_client = Rc::new(RefCell::new(VecDeque::new()));
      let to_server = Rc::new(RefCell::new(VecDeque::new()));
      let mut client = SslStream::new(
        connector.configure().unwrap().into_ssl(hostname).unwrap(),
        Pipe {
          incoming: to_client.clone(),
          outgoing: to_server.clone(),
        },
      )
      .unwrap();
      let mut server = SslStream::new(
        Ssl::new(&server_context).unwrap(),
        Pipe {
          incoming: to_server,
          outgoing: to_client,
        },
      )
      .unwrap();
      let mut client_done = false;
      let mut server_done = false;
      for _ in 0..16 {
        if !client_done {
          match client.connect() {
            Ok(()) => client_done = true,
            Err(error) if error.code() == ErrorCode::WANT_READ => {}
            Err(error) => {
              assert_eq!(error.code(), ErrorCode::SSL);
              let result = client.ssl().verify_result();
              assert_ne!(result, openssl::x509::X509VerifyResult::OK);
              return Err(result);
            }
          }
        }
        if !server_done {
          match server.accept() {
            Ok(()) => server_done = true,
            Err(error) if error.code() == ErrorCode::WANT_READ => {}
            Err(error) => {
              panic!("unexpected server handshake failure: {error}")
            }
          }
        }
        if client_done && server_done {
          return Ok(());
        }
      }
      panic!("in-memory TLS handshake did not complete");
    };

    assert!(handshake(tls_connector(None).unwrap(), "server.test").is_err());
    let mut config = tls_test_config(&cert, &key);
    assert!(
      handshake(tls_connector(Some(&config)).unwrap(), "server.test").is_ok()
    );
    config.verify = true;
    assert!(
      handshake(tls_connector(Some(&config)).unwrap(), "server.test").is_ok()
    );
    assert!(
      handshake(tls_connector(Some(&config)).unwrap(), "other.test").is_err()
    );
  }

  fn response_with_chunks(
    chunks: &[&'static [u8]],
  ) -> ntex::client::ClientResponse {
    let chunks: Vec<_> = chunks
      .iter()
      .map(|chunk| Ok(Bytes::from_static(chunk)))
      .collect();
    let res = TestResponse::builder().build();
    res.set_payload(Payload::from_stream(futures::stream::iter(chunks)));
    res
  }

  #[ntex::test]
  async fn res_stream_receiver_drop_releases_quiet_payload() {
    struct DropSignal(Option<futures::channel::oneshot::Sender<()>>);
    impl Drop for DropSignal {
      fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
          let _ = sender.send(());
        }
      }
    }

    for initial in [&b""[..], &b"{\"value\":"[..], &b"{\"value\":1}\n"[..]] {
      let (polled_tx, polled_rx) = futures::channel::oneshot::channel();
      let (dropped_tx, dropped_rx) = futures::channel::oneshot::channel();
      let guard = DropSignal(Some(dropped_tx));
      let mut polled_tx = Some(polled_tx);
      let mut initial = Some(Bytes::copy_from_slice(initial));
      let payload = futures::stream::poll_fn(move |_| {
        let _keep_alive = &guard;
        if let Some(bytes) = initial.take() {
          return std::task::Poll::Ready(Some(Ok(bytes)));
        }
        if let Some(sender) = polled_tx.take() {
          let _ = sender.send(());
        }
        std::task::Poll::Pending
      });
      let response = TestResponse::builder().build();
      response.set_payload(Payload::from_stream(payload));
      let receiver = NanocldClient::res_stream::<Value>(response).await;
      ntex::time::timeout(Millis(1000), polled_rx)
        .await
        .expect("reader must reach the quiet payload")
        .unwrap();
      drop(receiver);
      ntex::time::timeout(Millis(1000), dropped_rx)
        .await
        .expect(
          "receiver drop must release a quiet payload without another event",
        )
        .unwrap();
    }
  }

  #[ntex::test]
  async fn res_stream_decodes_coalesced_records() {
    let res = response_with_chunks(&[b"{\"value\":1}\n{\"value\":2}\n"]);
    let mut stream = NanocldClient::res_stream::<Value>(res).await;

    assert_eq!(stream.next().await.unwrap().unwrap(), json!({"value": 1}));
    assert_eq!(stream.next().await.unwrap().unwrap(), json!({"value": 2}));
    assert!(stream.next().await.is_none());
  }

  #[ntex::test]
  async fn res_stream_retains_fragmented_records_and_partial_suffix() {
    let res = response_with_chunks(&[
      b"{",
      b"\"value\":",
      b"",
      b"1}",
      b"\n{\"value\":2}\n{\"value\":",
      b"",
      b"3}",
      b"\n{\"value\":4",
    ]);
    let mut stream = NanocldClient::res_stream::<Value>(res).await;

    for value in 1..=3 {
      assert_eq!(
        stream.next().await.unwrap().unwrap(),
        json!({"value": value})
      );
    }
    assert!(stream.next().await.is_none());
  }

  #[ntex::test]
  async fn res_stream_stops_after_malformed_record() {
    let res =
      response_with_chunks(&[b"{\"value\":1}\ninvalid\n{\"value\":2}\n"]);
    let mut stream = NanocldClient::res_stream::<Value>(res).await;

    assert_eq!(stream.next().await.unwrap().unwrap(), json!({"value": 1}));
    let error = stream.next().await.unwrap().unwrap_err();
    assert!(error.to_string().contains("Unable to parse stream:"));
    assert!(stream.next().await.is_none());
  }
}
