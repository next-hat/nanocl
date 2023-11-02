use nanocl_error::http_client::HttpClientError;
use nanocl_stubs::secret::{Secret, SecretPartial, SecretUpdate, SecretQuery};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for secrets
  const SECRET_PATH: &str = "/secrets";

  /// ## List secrets
  ///
  /// List existing secrets in the system.
  ///
  /// ## Arguments
  ///
  /// * [query](Option) - The optional [query](SecretQuery)
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vector](Vec) of [secrets](Secret) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_secret(None).await;
  /// ```
  ///
  pub async fn list_secret(
    &self,
    query: Option<&SecretQuery>,
  ) -> Result<Vec<Secret>, HttpClientError> {
    let res = self.send_get(Self::SECRET_PATH, query).await?;
    Self::res_json(res).await
  }

  /// ## Create secret
  ///
  /// ## Arguments
  ///
  /// * [secret](SecretPartial) - The secret to create
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - [Secret](Secret) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  pub async fn create_secret(
    &self,
    item: &SecretPartial,
  ) -> Result<Secret, HttpClientError> {
    let res = self
      .send_post(Self::SECRET_PATH, Some(item), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// ## Patch secret
  ///
  /// Patch a secret by it's key to update it with new data
  ///
  /// ## Arguments
  ///
  /// * [secret](SecretUpdate) - The key of the secret to create
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - [Secret](Secret) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  pub async fn patch_secret(
    &self,
    item: &SecretUpdate,
  ) -> Result<Secret, HttpClientError> {
    let res = self
      .send_patch(Self::SECRET_PATH, Some(item), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// ## Inspect secret
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
  ///   * [Ok](Ok) - [Secret](Secret) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
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
        &format!("{}/{key}/inspect", Self::SECRET_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Delete a secret
  ///
  /// Delete a [secret](Secret) by it's key
  ///
  /// ## Arguments
  ///
  /// * [key](str) - The key of the [secret](Secret) to delete
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - If operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
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
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
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
