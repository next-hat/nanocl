use ntex::web::test::TestServer;
use ntex::http::client::{ClientRequest, ClientResponse};

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

  fn get(&self, url: &str) -> ClientRequest {
    self.srv.get(self.gen_url(url))
  }

  fn delete(&self, url: &str) -> ClientRequest {
    self
      .srv
      .delete(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  fn post(&self, url: &str) -> ClientRequest {
    self
      .srv
      .post(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  fn patch(&self, url: &str) -> ClientRequest {
    self
      .srv
      .patch(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  fn put(&self, url: &str) -> ClientRequest {
    self
      .srv
      .put(self.gen_url(url))
      .header("User-Agent", "nanocld_client")
  }

  fn head(&self, url: &str) -> ClientRequest {
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
      req = req.query(&query).unwrap()
    }
    req.send().await.unwrap()
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
      req = req.query(&query).unwrap();
    }
    match body {
      None => req.send().await.unwrap(),
      Some(body) => req.send_json(&body).await.unwrap(),
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
      req = req.query(&query).unwrap()
    }
    req.send().await.unwrap()
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
      req = req.query(&query).unwrap()
    }
    match body {
      None => req.send().await.unwrap(),
      Some(body) => req.send_json(&body).await.unwrap(),
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
      req = req.query(&query).unwrap()
    }
    req.send().await.unwrap()
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
