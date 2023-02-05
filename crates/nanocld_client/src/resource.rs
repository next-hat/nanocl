use nanocl_stubs::resource::{
  Resource, ResourcePartial, ResourceConfig, ResourceQuery,
};

use super::http_client::NanoclClient;
use super::error::{NanoclClientError, is_api_error};

impl NanoclClient {
  /// ## List resources
  ///
  /// List all existing resources
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Vec<Resource>) - The resources
  ///   * [Err](NanoclClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let namespaces = client.list_resource().await;
  /// ```
  ///
  pub async fn list_resource(
    &self,
    query: Option<ResourceQuery>,
  ) -> Result<Vec<Resource>, NanoclClientError> {
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
  ///   * [Err](NanoclClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  /// use nanocl_stubs::resource::ResourceKind;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
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
  ) -> Result<Resource, NanoclClientError> {
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
  ///   * [Err](NanoclClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let resource = client.inspect_resource("my-resource").await;
  /// ```
  ///
  pub async fn inspect_resource(
    &self,
    key: &str,
  ) -> Result<Resource, NanoclClientError> {
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
  ///   * [Err](NanoclClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let resource = client.patch_resource("my-resource", serde_json::json!({})).await;
  /// ```
  ///
  pub async fn patch_resource(
    &self,
    key: &str,
    config: &serde_json::Value,
  ) -> Result<Resource, NanoclClientError> {
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
  ///   * [Err](NanoclClientError) - An error if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let resource = client.delete_resource("my-resource").await;
  /// ```
  ///
  pub async fn delete_resource(
    &self,
    key: &str,
  ) -> Result<(), NanoclClientError> {
    let mut res = self.delete(format!("/resources/{key}")).send().await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    Ok(())
  }

  pub async fn list_history_resource(
    &self,
    key: &str,
  ) -> Result<Vec<ResourceConfig>, NanoclClientError> {
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
  ) -> Result<Resource, NanoclClientError> {
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
  use nanocl_stubs::resource::ResourceKind;

  use super::*;

  #[ntex::test]
  async fn test_basic() {
    let client = NanoclClient::connect_with_unix_default();

    // list
    client.list_resource(None).await.unwrap();

    // create
    let resource = client
      .create_resource(&ResourcePartial {
        name: "my-resource".into(),
        kind: ResourceKind::ProxyRule,
        config: serde_json::json!({}),
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
    let resource = client
      .patch_resource("my-resource", &serde_json::json!({"config": "gg"}))
      .await
      .unwrap();

    assert_eq!(resource.name, "my-resource");
    assert_eq!(resource.kind, ResourceKind::ProxyRule);
    assert_eq!(resource.config, serde_json::json!({"config": "gg"}));

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
