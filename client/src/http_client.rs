use ntex::rt;
use ntex::http::Client;
use ntex::http::client::{Connector, ClientRequest};

pub struct NanoclClient {
  client: Client,
  url: String,
}

impl NanoclClient {
  pub async fn connect_with_unix_default() -> Self {
    let client = Client::build()
      .connector(
        Connector::default()
          .connector(ntex::service::fn_service(|_| async {
            Ok::<_, _>(rt::unix_connect("/run/nanocl/nanocl.sock").await?)
          }))
          .finish(),
      )
      .timeout(ntex::time::Millis::from_secs(20))
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
}
