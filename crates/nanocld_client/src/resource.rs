use nanocl_error::http_client::{HttpClientResult, HttpClientError};

use nanocl_error::io::IoError;
use nanocl_stubs::generic::{GenericFilter, GenericListQuery};
use nanocl_stubs::resource::{
  Resource, ResourcePartial, ResourceSpec, ResourceUpdate,
};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for resources
  const RESOURCE_PATH: &'static str = "/resources";

  /// List existing resources in the system.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_resource().await;
  /// ```
  ///
  pub async fn list_resource(
    &self,
    query: Option<&GenericFilter>,
  ) -> HttpClientResult<Vec<Resource>> {
    let query = query.cloned().unwrap_or_default();
    let query = GenericListQuery::try_from(query).map_err(|err| {
      HttpClientError::IoError(IoError::invalid_data(
        "Query".to_owned(),
        err.to_string(),
      ))
    })?;
    let res = self.send_get(Self::RESOURCE_PATH, Some(query)).await?;
    Self::res_json(res).await
  }

  /// Create a new resource from a partial resource in the system.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocl_stubs::resource::ResourceKind;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.create_resource(&ResourcePartial {
  ///   name: "my-resource".into(),
  ///   kind: String::from("Custom")s,
  ///   // Your data
  ///   data: serde_json::json!({}),
  /// }).await;
  /// ```
  pub async fn create_resource(
    &self,
    data: &ResourcePartial,
  ) -> HttpClientResult<Resource> {
    let res = self
      .send_post(Self::RESOURCE_PATH, Some(data), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// Inspect an existing resource
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.inspect_resource("my-resource").await;
  /// ```
  pub async fn inspect_resource(
    &self,
    key: &str,
  ) -> HttpClientResult<Resource> {
    let res = self
      .send_get(&format!("{}/{key}", Self::RESOURCE_PATH), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// Patch an existing resource
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.patch_resource("my-resource", serde_json::json!({})).await;
  /// ```
  pub async fn put_resource(
    &self,
    key: &str,
    config: &ResourceUpdate,
  ) -> HttpClientResult<Resource> {
    let res = self
      .send_patch(
        &format!("{}/{key}", Self::RESOURCE_PATH),
        Some(config),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// Delete an existing resource
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_resource("my-resource").await;
  /// ```
  pub async fn delete_resource(&self, key: &str) -> HttpClientResult<()> {
    self
      .send_delete(&format!("{}/{key}", Self::RESOURCE_PATH), None::<String>)
      .await?;
    Ok(())
  }

  /// List history of an existing resource
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_history_resource("my-resource").await;
  /// ```
  pub async fn list_history_resource(
    &self,
    key: &str,
  ) -> HttpClientResult<Vec<ResourceSpec>> {
    let res = self
      .send_get(
        &format!("{}/{key}/histories", Self::RESOURCE_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// Revert a resource to a previous version
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let history = client.list_history_resource("my-resource").await.unwrap().first().unwrap();
  /// let res = client.revert_resource("my-resource", history.key).await;
  /// ```
  pub async fn revert_resource(
    &self,
    name: &str,
    key: &str,
  ) -> HttpClientResult<Resource> {
    let res = self
      .send_patch(
        &format!("{}/{name}/histories/{key}/revert", Self::RESOURCE_PATH),
        None::<String>,
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }
}
