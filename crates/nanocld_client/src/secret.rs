use nanocl_error::io::IoError;
use nanocl_error::http_client::{HttpClientError, HttpClientResult};

use nanocl_stubs::generic::{GenericFilter, GenericListQuery};
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
    let query = query.cloned().unwrap_or_default();
    let query = GenericListQuery::try_from(query).map_err(|err| {
      HttpClientError::IoError(IoError::invalid_data(
        "Query".to_owned(),
        err.to_string(),
      ))
    })?;
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
    item: &SecretUpdate,
  ) -> HttpClientResult<Secret> {
    let res = self
      .send_patch(Self::SECRET_PATH, Some(item), None::<String>)
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
  use super::*;

  #[ntex::test]
  async fn basic() {
    const SECRET_KEY: &str = "secret-test";
    let client = NanocldClient::connect_to("http://nanocl.internal:8585", None);
    client.list_secret(None).await.unwrap();
    let secret = SecretPartial {
      key: SECRET_KEY.to_owned(),
      kind: "generic".to_owned(),
      data: serde_json::json!({"key": "value"}),
      metadata: None,
      immutable: None,
    };
    let secret = client.create_secret(&secret).await.unwrap();
    assert_eq!(secret.key, SECRET_KEY);
    let secret = client.inspect_secret(SECRET_KEY).await.unwrap();
    assert_eq!(secret.key, SECRET_KEY);
    client.delete_secret(SECRET_KEY).await.unwrap();
  }
}
