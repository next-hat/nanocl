use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::namespace::{Namespace, NamespaceSummary, NamespaceInspect};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for namespaces
  const NAMESPACE_PATH: &'static str = "/namespaces";

  /// ## List namespace
  ///
  /// List all namespaces from the system
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Vec](Vec) of [NamespaceSummary](NamespaceSummary)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_namespace().await;
  /// ```
  ///
  pub async fn list_namespace(
    &self,
  ) -> HttpClientResult<Vec<NamespaceSummary>> {
    let res = self.send_get(Self::NAMESPACE_PATH, None::<String>).await?;
    Self::res_json(res).await
  }

  /// ## Create namespace
  ///
  /// Create a namespace by it's name
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the namespace to create
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Namespace](Namespace)
  ///
  pub async fn create_namespace(
    &self,
    name: &str,
  ) -> HttpClientResult<Namespace> {
    let new_item = Namespace { name: name.into() };
    let res = self
      .send_post(Self::NAMESPACE_PATH, Some(new_item), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// ## Inspect namespace
  ///
  /// Inspect a namespace by it's name to get detailed information about it.
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the namespace to inspect
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [NamespaceInspect](NamespaceInspect)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.inspect_namespace("my-namespace").await;
  /// ```
  ///
  pub async fn inspect_namespace(
    &self,
    name: &str,
  ) -> HttpClientResult<NamespaceInspect> {
    let res = self
      .send_get(
        &format!("{}/{name}/inspect", Self::NAMESPACE_PATH),
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
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_namespace("my-namespace").await;
  /// ```
  ///
  pub async fn delete_namespace(&self, name: &str) -> HttpClientResult<()> {
    self
      .send_delete(&format!("{}/{name}", Self::NAMESPACE_PATH), None::<String>)
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn basic() {
    const NAMESPACE: &str = "clientnt";
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    client.list_namespace().await.unwrap();
    let namespace = client.create_namespace(NAMESPACE).await.unwrap();
    assert_eq!(namespace.name, NAMESPACE);
    let namespace = client.inspect_namespace(NAMESPACE).await.unwrap();
    assert_eq!(namespace.name, NAMESPACE);
    client.delete_namespace(NAMESPACE).await.unwrap();
  }
}
