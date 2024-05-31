use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::generic::GenericFilter;
use nanocl_stubs::secret::{Secret, SecretPartial, SecretUpdate};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for secrets
  const SECRET_PATH: &'static str = "/secrets";

  /// List existing secrets in the system.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_secret(None).await;
  /// ```
  pub async fn list_secret(
    &self,
    query: Option<&GenericFilter>,
  ) -> HttpClientResult<Vec<Secret>> {
    let query = Self::convert_query(query)?;
    let res = self.send_get(Self::SECRET_PATH, Some(&query)).await?;
    Self::res_json(res).await
  }

  /// Create a new secret
  pub async fn create_secret(
    &self,
    item: &SecretPartial,
  ) -> HttpClientResult<Secret> {
    let res = self
      .send_post(Self::SECRET_PATH, Some(item), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// Patch a secret by it's key to update it with new data
  pub async fn patch_secret(
    &self,
    key: &str,
    item: &SecretUpdate,
  ) -> HttpClientResult<Secret> {
    let res = self
      .send_patch(
        &format!("{}/{key}", Self::SECRET_PATH),
        Some(item),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// Inspect a secret by it's key to get more information about it
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let secret = client.inspect_secret("my-secret").await?;
  /// ```
  pub async fn inspect_secret(&self, key: &str) -> HttpClientResult<Secret> {
    let res = self
      .send_get(
        &format!("{}/{key}/inspect", Self::SECRET_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// Delete a secret by it's key
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// client.delete_secret("my-secret").await?;
  /// ```
  pub async fn delete_secret(&self, key: &str) -> HttpClientResult<()> {
    self
      .send_delete(&format!("{}/{key}", Self::SECRET_PATH), None::<String>)
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use crate::ConnectOpts;

  use super::*;

  #[ntex::test]
  async fn basic() {
    const SECRET_NAME: &str = "secret-test";
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    client.list_secret(None).await.unwrap();
    let secret = SecretPartial {
      name: SECRET_NAME.to_owned(),
      kind: "gen.io/generic".to_owned(),
      data: serde_json::json!({"key": "value"}),
      metadata: None,
      immutable: None,
    };
    let secret = client.create_secret(&secret).await.unwrap();
    assert_eq!(secret.name, SECRET_NAME);
    let secret = client.inspect_secret(SECRET_NAME).await.unwrap();
    assert_eq!(secret.name, SECRET_NAME);
    client.delete_secret(SECRET_NAME).await.unwrap();
  }
}
