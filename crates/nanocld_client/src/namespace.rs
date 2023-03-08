use nanocl_stubs::namespace::{Namespace, NamespaceSummary, NamespaceInspect};

use super::http_client::NanocldClient;

use super::error::NanocldClientError;

impl NanocldClient {
  /// ## List all namespaces
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - A [Vec](Vec) of [namespaces](NamespaceSummary)
  ///   * [Err](NanocldClientError) - The namespaces could not be listed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let namespaces = client.list_namespace().await;
  /// ```
  ///
  pub async fn list_namespace(
    &self,
  ) -> Result<Vec<NamespaceSummary>, NanocldClientError> {
    let res = self
      .send_get(format!("/{}/namespaces", &self.version), None::<String>)
      .await?;

    Self::res_json(res).await
  }

  /// ## Create a new namespace
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the namespace to create
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The created [namespace](Namespace)
  ///   * [Err](NanocldClientError) - The namespace could not be created
  ///
  pub async fn create_namespace(
    &self,
    name: &str,
  ) -> Result<Namespace, NanocldClientError> {
    let new_item = Namespace { name: name.into() };
    let res = self
      .send_post(
        format!("/{}/namespaces", &self.version),
        Some(new_item),
        None::<String>,
      )
      .await?;

    Self::res_json(res).await
  }

  /// ## Inspect a namespace
  ///
  /// Inspect a namespace by it's name to get more information about it
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the namespace to inspect
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The desired [namespace](NamespaceInspect)
  ///   * [Err](NanocldClientError) - The namespace could not be inspected
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let namespace = client.inspect_namespace("my-namespace").await?;
  /// ```
  ///
  pub async fn inspect_namespace(
    &self,
    name: &str,
  ) -> Result<NamespaceInspect, NanocldClientError> {
    let res = self
      .send_get(
        format!("/{}/namespaces/{name}/inspect", &self.version),
        None::<String>,
      )
      .await?;

    Self::res_json(res).await
  }

  /// ## Delete a namespace
  ///
  /// Delete a namespace by it's name
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the namespace to delete
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The namespace was deleted
  ///   * [Err](NanocldClientError) - The namespace could not be deleted
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// client.delete_namespace("my-namespace").await?;
  /// ```
  ///
  pub async fn delete_namespace(
    &self,
    name: &str,
  ) -> Result<(), NanocldClientError> {
    self
      .send_delete(
        format!("/{}/namespaces/{name}", &self.version),
        None::<String>,
      )
      .await?;

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn test_basic() {
    const NAMESPACE: &str = "clientnt";
    let client = NanocldClient::connect_with_unix_default();

    client.list_namespace().await.unwrap();

    let namespace = client.create_namespace(NAMESPACE).await.unwrap();
    assert_eq!(namespace.name, NAMESPACE);

    let namespace = client.inspect_namespace(NAMESPACE).await.unwrap();
    assert_eq!(namespace.name, NAMESPACE);

    client.delete_namespace(NAMESPACE).await.unwrap();
  }
}
