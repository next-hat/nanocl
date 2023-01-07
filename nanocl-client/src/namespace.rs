use nanocl_models::namespace::{Namespace, NamespaceSummary};

use super::http_client::NanoclClient;

use super::error::{NanoclClientError, is_api_error};

impl NanoclClient {
  /// ## List all namespaces
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - A [Vec](Vec) of [namespaces](NamespaceSummary)
  ///   * [Err](NanoclClientError) - The namespaces could not be listed
  ///
  /// ## Example
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let namespaces = client.list_namespace().await;
  /// ```
  ///
  pub async fn list_namespace(
    &self,
  ) -> Result<Vec<NamespaceSummary>, NanoclClientError> {
    let mut res = self.get(String::from("/namespaces")).send().await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<NamespaceSummary>>().await?;
    Ok(items)
  }

  /// ## Create a new namespace
  ///
  /// ## Arguments
  /// * [name](str) - The name of the namespace to create
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The created [namespace](Namespace)
  ///   * [Err](NanoclClientError) - The namespace could not be created
  ///
  pub async fn create_namespace(
    &self,
    name: &str,
  ) -> Result<Namespace, NanoclClientError> {
    let new_item = Namespace { name: name.into() };
    let mut res = self
      .post(String::from("/namespaces"))
      .send_json(&new_item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<Namespace>().await?;

    Ok(item)
  }

  /// ## Inspect a namespace
  /// Inspect a namespace by it's name to get more information about it
  ///
  /// ## Arguments
  /// * [name](str) - The name of the namespace to inspect
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The desired [namespace](Namespace)
  ///   * [Err](NanoclClientError) - The namespace could not be inspected
  ///
  /// ## Example
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let namespace = client.inspect_namespace("my-namespace").await?;
  /// ```
  ///
  pub async fn inspect_namespace(
    &self,
    name: &str,
  ) -> Result<Namespace, NanoclClientError> {
    let mut res = self
      .get(format!("/namespaces/{name}/inspect", name = name))
      .send()
      .await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<Namespace>().await?;

    Ok(item)
  }

  /// ## Delete a namespace
  /// Delete a namespace by it's name
  ///
  /// ## Arguments
  /// * [name](str) - The name of the namespace to delete
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The namespace was deleted
  ///   * [Err](NanoclClientError) - The namespace could not be deleted
  ///
  /// ## Example
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// client.delete_namespace("my-namespace").await?;
  /// ```
  ///
  pub async fn delete_namespace(
    &self,
    name: &str,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .delete(format!("/namespaces/{name}", name = name))
      .send()
      .await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn test_basic() {
    const NAMESPACE: &str = "clientnt";
    let client = NanoclClient::connect_with_unix_default().await;

    client.list_namespace().await.unwrap();

    let namespace = client.create_namespace(NAMESPACE).await.unwrap();
    assert_eq!(namespace.name, NAMESPACE);

    let namespace = client.inspect_namespace(NAMESPACE).await.unwrap();
    assert_eq!(namespace.name, NAMESPACE);

    client.delete_namespace(NAMESPACE).await.unwrap();
  }
}
