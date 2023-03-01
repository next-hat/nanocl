use nanocl_stubs::resource::{
  Resource, ResourcePartial, ResourceConfig, ResourceQuery,
};

use super::http_client::NanocldClient;
use super::error::{NanocldClientError, is_api_error};

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
    let mut req = self.get("/resources".into());
    if let Some(query) = query {
      req = req.query(&query)?;
    }
    let mut res = req.send().await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let resources = res.json::<Vec<Resource>>().await?;
    Ok(resources)
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
    let mut res = self.post("/resources".into()).send_json(data).await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let resource = res.json::<Resource>().await?;
    Ok(resource)
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
    let mut res = self.get(format!("/resources/{key}")).send().await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let resource = res.json::<Resource>().await?;
    Ok(resource)
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
    let mut res = self
      .patch(format!("/resources/{key}"))
      .send_json(config)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let resource = res.json::<Resource>().await?;
    Ok(resource)
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
    let mut res = self.delete(format!("/resources/{key}")).send().await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    Ok(())
  }

  pub async fn list_history_resource(
    &self,
    key: &str,
  ) -> Result<Vec<ResourceConfig>, NanocldClientError> {
    let mut res = self
      .get(format!("/resources/{key}/histories"))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let history = res.json::<Vec<ResourceConfig>>().await?;
    Ok(history)
  }

  pub async fn reset_resource(
    &self,
    name: &str,
    key: &str,
  ) -> Result<Resource, NanocldClientError> {
    let mut res = self
      .patch(format!("/resources/{name}/histories/{key}/reset"))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let resource = res.json::<Resource>().await?;
    Ok(resource)
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
