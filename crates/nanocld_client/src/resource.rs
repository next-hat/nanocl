use nanocl_stubs::resource::{
  Resource, ResourcePartial, ResourceConfig, ResourceQuery,
};

use super::http_client::NanocldClient;
use super::error::NanocldClientError;

impl NanocldClient {
  /// ## List resources
  ///
  /// List all existing resources
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Vec<Resource>) - The resources
  ///   * [Err](NanocldClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let namespaces = client.list_resource().await;
  /// ```
  ///
  pub async fn list_resource(
    &self,
    query: Option<ResourceQuery>,
  ) -> Result<Vec<Resource>, NanocldClientError> {
    let res = self
      .send_get(format!("/{}/resources", &self.version), query)
      .await?;

    Self::res_json(res).await
  }

  /// ## Create resource
  ///
  /// Create a new resource
  ///
  /// ## Arguments
  ///
  /// * [data](ResourcePartial) - The data of the resource to create
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Resource) - The created resource
  ///   * [Err](NanocldClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocl_stubs::resource::ResourceKind;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let resource = client.create_resource(&ResourcePartial {
  ///   name: "my-resource".into(),
  ///   kind: ResourceKind::ProxyRules,
  ///   // Your config
  ///   config: serde_json::json!({}),
  /// }).await;
  /// ```
  ///
  pub async fn create_resource(
    &self,
    data: &ResourcePartial,
  ) -> Result<Resource, NanocldClientError> {
    let res = self
      .send_post(
        format!("/{}/resources", &self.version),
        Some(data),
        None::<String>,
      )
      .await?;

    Self::res_json(res).await
  }

  /// ## Inspect resource
  ///
  /// Inspect an existing resource
  ///
  /// ## Arguments
  ///
  /// * [key](str) - The key of the resource to inspect
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Resource) - The inspected resource
  ///   * [Err](NanocldClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let resource = client.inspect_resource("my-resource").await;
  /// ```
  ///
  pub async fn inspect_resource(
    &self,
    key: &str,
  ) -> Result<Resource, NanocldClientError> {
    let res = self
      .send_get(
        format!("/{}/resources/{key}", &self.version),
        None::<String>,
      )
      .await?;

    Self::res_json(res).await
  }

  /// ## Patch resource
  ///
  /// Patch an existing resource
  ///
  /// ## Arguments
  ///
  /// * [key](str) - The key of the resource to patch
  /// * [data](ResourcePartial) - The data to patch
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Resource) - The patched resource
  ///   * [Err](NanocldClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let resource = client.patch_resource("my-resource", serde_json::json!({})).await;
  /// ```
  ///
  pub async fn patch_resource(
    &self,
    key: &str,
    config: &serde_json::Value,
  ) -> Result<Resource, NanocldClientError> {
    let res = self
      .send_patch(
        format!("/{}/resources/{key}", &self.version),
        Some(config),
        None::<String>,
      )
      .await?;

    Self::res_json(res).await
  }

  /// ## Delete resource
  ///
  /// Delete an existing resource
  ///
  /// ## Arguments
  ///
  /// * [key](str) - The key of the resource to delete
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok(())) - The operation succeeded
  ///   * [Err](NanocldClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let resource = client.delete_resource("my-resource").await;
  /// ```
  ///
  pub async fn delete_resource(
    &self,
    key: &str,
  ) -> Result<(), NanocldClientError> {
    self
      .send_delete(
        format!("/{}/resources/{key}", &self.version),
        None::<String>,
      )
      .await?;

    Ok(())
  }

  pub async fn list_history_resource(
    &self,
    key: &str,
  ) -> Result<Vec<ResourceConfig>, NanocldClientError> {
    let res = self
      .send_get(
        format!("/{}/resources/{key}/histories", &self.version),
        None::<String>,
      )
      .await?;

    Self::res_json(res).await
  }

  pub async fn reset_resource(
    &self,
    name: &str,
    key: &str,
  ) -> Result<Resource, NanocldClientError> {
    let res = self
      .send_patch(
        format!("/{}/resources/{name}/histories/{key}/reset", &self.version),
        None::<String>,
        None::<String>,
      )
      .await?;

    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::resource::{
    ResourceKind, ResourceProxyRule, ProxyRuleStream, ProxyRule,
    ProxyStreamProtocol, ProxyTarget,
  };

  use super::*;

  #[ntex::test]
  async fn test_basic() {
    let client = NanocldClient::connect_with_unix_default();

    // list
    client.list_resource(None).await.unwrap();

    let config = serde_json::to_value(ResourceProxyRule {
      watch: vec!["random-cargo".into()],
      rule: ProxyRule::Stream(ProxyRuleStream {
        network: "Public".into(),
        protocol: ProxyStreamProtocol::Tcp,
        port: 1234,
        ssl: None,
        target: ProxyTarget {
          key: "random-cargo".into(),
          port: 1234,
        },
      }),
    })
    .unwrap();

    // create
    let resource = client
      .create_resource(&ResourcePartial {
        name: "my-resource".into(),
        kind: ResourceKind::ProxyRule,
        config: config.clone(),
      })
      .await
      .unwrap();

    assert_eq!(resource.name, "my-resource");
    assert_eq!(resource.kind, ResourceKind::ProxyRule);

    // inspect
    let resource = client.inspect_resource("my-resource").await.unwrap();
    assert_eq!(resource.name, "my-resource");
    assert_eq!(resource.kind, ResourceKind::ProxyRule);

    // patch
    let resource = client.patch_resource("my-resource", &config).await.unwrap();

    assert_eq!(resource.name, "my-resource");
    assert_eq!(resource.kind, ResourceKind::ProxyRule);

    // history
    let history = client.list_history_resource("my-resource").await.unwrap();
    assert!(history.len() > 1);

    // reset
    let _ = client
      .reset_resource("my-resource", &history[0].key.to_string())
      .await
      .unwrap();

    // delete
    client.delete_resource("my-resource").await.unwrap();
  }
}
