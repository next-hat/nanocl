use nanocl_error::http_client::HttpClientError;
use nanocl_stubs::secret::{Secret, SecretPartial, SecretUpdate, SecretQuery};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## List all secrets
  ///
  /// ## Arguments
  ///
  /// * [query](SecretQuery) - Query to filter secrets
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - A [Vec](Vec) of [secrets](SecretSummary)
  ///   * [Err](HttpClientError) - The secrets could not be listed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let secrets = client.list_secret(None).await;
  /// ```
  ///
  pub async fn list_secret(
    &self,
    query: Option<SecretQuery>,
  ) -> Result<Vec<Secret>, HttpClientError> {
    let res = self
      .send_get(format!("/{}/secrets", &self.version), query)
      .await?;
    Self::res_json(res).await
  }

  /// ## Create a new secret
  ///
  /// ## Arguments
  ///
  /// * [secret](SecretPartial) - The key of the secret to create
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The created [secret](Secret)
  ///   * [Err](HttpClientError) - The secret could not be created
  ///
  pub async fn create_secret(
    &self,
    item: &SecretPartial,
  ) -> Result<Secret, HttpClientError> {
    let res = self
      .send_post(
        format!("/{}/secrets", &self.version),
        Some(item),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Patch a secret
  ///
  /// ## Arguments
  ///
  /// * [secret](SecretUpdate) - The key of the secret to create
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The created [secret](Secret)
  ///   * [Err](HttpClientError) - The secret could not be created
  ///
  pub async fn patch_secret(
    &self,
    item: &SecretUpdate,
  ) -> Result<Secret, HttpClientError> {
    let res = self
      .send_patch(
        format!("/{}/secrets", &self.version),
        Some(item),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Inspect a secret
  ///
  /// Inspect a secret by it's key to get more information about it
  ///
  /// ## Arguments
  ///
  /// * [key](str) - The key of the secret to inspect
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The desired [secret](SecretInspect)
  ///   * [Err](HttpClientError) - The secret could not be inspected
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let secret = client.inspect_secret("my-secret").await?;
  /// ```
  ///
  pub async fn inspect_secret(
    &self,
    key: &str,
  ) -> Result<Secret, HttpClientError> {
    let res = self
      .send_get(
        format!("/{}/secrets/{key}/inspect", &self.version),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Delete a secret
  ///
  /// Delete a secret by it's key
  ///
  /// ## Arguments
  ///
  /// * [key](str) - The key of the secret to delete
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The secret was deleted
  ///   * [Err](HttpClientError) - The secret could not be deleted
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// client.delete_secret("my-secret").await?;
  /// ```
  ///
  pub async fn delete_secret(&self, key: &str) -> Result<(), HttpClientError> {
    self
      .send_delete(format!("/{}/secrets/{key}", &self.version), None::<String>)
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
    let client = NanocldClient::connect_to("http://localhost:8585", None);
    client.list_secret(None).await.unwrap();
    let secret = SecretPartial {
      key: SECRET_KEY.to_string(),
      kind: "generic".to_string(),
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
