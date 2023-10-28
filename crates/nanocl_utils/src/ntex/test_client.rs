use ntex::web::test::TestServer;
use ntex::http::client::{ClientRequest, ClientResponse};

#[macro_export]
macro_rules! test_status_code {
  ($expected:expr,$current:expr,$context:expr) => {{
    assert_eq!(
      $expected, $current,
      "Expect {} to return status {} got: {}",
      $context, $expected, $current,
    );
  }};
}

pub use test_status_code;

pub struct TestClient {
  srv: TestServer,
  version: String,
}

impl TestClient {
  pub fn new(srv: TestServer, version: &str) -> Self {
    Self {
      srv,
      version: version.to_owned(),
    }
  }

  fn gen_url(&self, url: &str) -> String {
    format!("v{}{url}", self.version)
  }

  pub fn get(&self, url: &str) -> ClientRequest {
    self.srv.get(self.gen_url(url))
  }

  pub fn delete(&self, url: &str) -> ClientRequest {
    self
      .srv
      .delete(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  pub fn post(&self, url: &str) -> ClientRequest {
    self
      .srv
      .post(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  pub fn patch(&self, url: &str) -> ClientRequest {
    self
      .srv
      .patch(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  pub fn put(&self, url: &str) -> ClientRequest {
    self
      .srv
      .put(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  pub fn head(&self, url: &str) -> ClientRequest {
    self
      .srv
      .head(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  pub async fn send_get<Q>(&self, url: &str, query: Option<Q>) -> ClientResponse
  where
    Q: serde::Serialize,
  {
    let mut req = self.get(url);
    if let Some(query) = query {
      req = req.query(&query).unwrap_or_else(|err| {
        panic!("Failed to serialize query GET {url}: {err}")
      })
    }
    req
      .send()
      .await
      .unwrap_or_else(|err| panic!("Failed to send GET {url}: {err}"))
  }

  pub async fn send_post<Q, B>(
    &self,
    url: &str,
    body: Option<B>,
    query: Option<Q>,
  ) -> ClientResponse
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.post(url);
    if let Some(query) = query {
      req = req.query(&query).unwrap_or_else(|err| {
        panic!("Failed to serialize query POST {url}: {err}")
      });
    }
    match body {
      None => req
        .send()
        .await
        .unwrap_or_else(|err| panic!("Failed to send POST {url}: {err}")),
      Some(body) => req.send_json(&body).await.unwrap_or_else(|err| {
        panic!("Failed to send with body POST {url}: {err}")
      }),
    }
  }

  pub async fn send_delete<Q>(
    &self,
    url: &str,
    query: Option<Q>,
  ) -> ClientResponse
  where
    Q: serde::Serialize,
  {
    let mut req = self.delete(url);
    if let Some(query) = query {
      req = req.query(&query).unwrap_or_else(|err| {
        panic!("Failed to serialize query DELETE {url}: {err}")
      })
    }
    req
      .send()
      .await
      .unwrap_or_else(|err| panic!("Failed to send DELETE {url}: {err}"))
  }

  pub async fn send_patch<B, Q>(
    &self,
    url: &str,
    body: Option<B>,
    query: Option<Q>,
  ) -> ClientResponse
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.patch(url);
    if let Some(query) = query {
      req = req.query(&query).unwrap_or_else(|err| {
        panic!("Failed to serialize query PATCH {url}: {err}")
      })
    }
    match body {
      None => req
        .send()
        .await
        .unwrap_or_else(|err| panic!("Failed to send PATCH {url}: {err}")),
      Some(body) => req.send_json(&body).await.unwrap_or_else(|err| {
        panic!("Failed to send with body PATCH {url}: {err}")
      }),
    }
  }

  pub async fn send_head<Q>(
    &self,
    url: &str,
    query: Option<Q>,
  ) -> ClientResponse
  where
    Q: serde::Serialize,
  {
    let mut req = self.head(url);
    if let Some(query) = query {
      req = req.query(&query).unwrap_or_else(|err| {
        panic!("Failed to serialize query HEAD {url}: {err}")
      })
    }
    req
      .send()
      .await
      .unwrap_or_else(|err| panic!("Failed to send HEAD {url}: {err}"))
  }

  pub async fn send_put<B, Q>(
    &self,
    url: &str,
    body: Option<B>,
    query: Option<Q>,
  ) -> ClientResponse
  where
    B: serde::Serialize,
    Q: serde::Serialize,
  {
    let mut req = self.put(url);
    if let Some(query) = query {
      req = req.query(&query).unwrap()
    }
    match body {
      None => req.send().await.unwrap(),
      Some(body) => req.send_json(&body).await.unwrap(),
    }
  }

  pub async fn res_json<R>(mut res: ClientResponse) -> R
  where
    R: serde::de::DeserializeOwned + Send + 'static,
  {
    res.json::<R>().limit(20_000_000).await.unwrap()
  }
}
