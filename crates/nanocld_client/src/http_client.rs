use ntex::rt;
use ntex::channel::mpsc::Receiver;
use ntex::http::{Client, StatusCode};
use ntex::http::client::{Connector, ClientRequest, ClientResponse};
use futures::{StreamExt, TryStreamExt};

use crate::error::ApiError;

#[derive(Clone)]
pub struct NanoclClient {
  client: Client,
  url: String,
}

impl NanoclClient {
  pub fn connect_with_unix_default() -> Self {
    let client = Client::build()
      .connector(
        Connector::default()
          .connector(ntex::service::fn_service(|_| async {
            Ok::<_, _>(rt::unix_connect("/run/nanocl/nanocl.sock").await?)
          }))
          .timeout(ntex::time::Millis::from_secs(10))
          .finish(),
      )
      .timeout(ntex::time::Millis::from_secs(10))
      .finish();

    NanoclClient {
      client,
      url: String::from("http://localhost"),
    }
  }

  fn gen_url(&self, url: String) -> String {
    self.url.to_owned() + &url
  }

  pub(crate) fn get(&self, url: String) -> ClientRequest {
    self.client.get(self.gen_url(url))
  }

  pub(crate) fn delete(&self, url: String) -> ClientRequest {
    self.client.delete(self.gen_url(url))
  }

  pub(crate) fn post(&self, url: String) -> ClientRequest {
    self.client.post(self.gen_url(url))
  }

  pub(crate) fn patch(&self, url: String) -> ClientRequest {
    self.client.patch(self.gen_url(url))
  }

  pub(crate) fn put(&self, url: String) -> ClientRequest {
    self.client.put(self.gen_url(url))
  }

  pub(crate) async fn stream<T>(
    &self,
    res: ClientResponse,
  ) -> Receiver<Result<T, ApiError>>
  where
    T: serde::de::DeserializeOwned + Send + 'static,
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
        let t = match serde_json::from_slice::<T>(&payload) {
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
