use nanocl_error::{
  io::IoError,
  http_client::{HttpClientError, HttpClientResult},
};

use nanocl_stubs::{
  generic::{GenericFilter, GenericListQuery},
  resource_kind::{
    ResourceKind, ResourceKindInspect, ResourceKindPartial, ResourceKindVersion,
  },
};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for resource kinds
  const RESOURCE_KIND_PATH: &'static str = "/resource/kinds";

  /// List existing resource kinds in the system.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_resource_kind(None).await;
  /// ```
  pub async fn list_resource_kind(
    &self,
    query: Option<&GenericFilter>,
  ) -> HttpClientResult<Vec<ResourceKind>> {
    let query = query.cloned().unwrap_or_default();
    let query = GenericListQuery::try_from(query).map_err(|err| {
      HttpClientError::IoError(IoError::invalid_data(
        "Query".to_owned(),
        err.to_string(),
      ))
    })?;
    let res = self
      .send_get(Self::RESOURCE_KIND_PATH, Some(&query))
      .await?;
    Self::res_json(res).await
  }

  /// Create a new secret
  pub async fn create_resource_kind(
    &self,
    item: &ResourceKindPartial,
  ) -> HttpClientResult<ResourceKind> {
    let res = self
      .send_post(Self::RESOURCE_KIND_PATH, Some(item), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// Inspect a resource kind by it's key to get more information about it
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let secret = client.inspect_resource_kind("ncproxy.io/rule").await?;
  /// ```
  pub async fn inspect_resource_kind(
    &self,
    key: &str,
  ) -> HttpClientResult<ResourceKindInspect> {
    let res = self
      .send_get(
        &format!("{}/{key}/inspect", Self::RESOURCE_KIND_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// Inspect a version of resource kind
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let secret = client.inspect_resource_kind_version("ncproxy.io/rule", "v0.1").await?;
  /// ```
  pub async fn inspect_resource_kind_version(
    &self,
    key: &str,
    version: &str,
  ) -> HttpClientResult<ResourceKindVersion> {
    let res = self
      .send_get(
        &format!(
          "{}/{key}/version/{version}/inspect",
          Self::RESOURCE_KIND_PATH
        ),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// Delete a resource kind by it's key
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// client.delete_resource_kind("ncproxy.io/rule").await?;
  /// ```
  pub async fn delete_resource_kind(&self, key: &str) -> HttpClientResult<()> {
    self
      .send_delete(
        &format!("{}/{key}", Self::RESOURCE_KIND_PATH),
        None::<String>,
      )
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use crate::ConnectOpts;

  use super::*;

  use nanocl_stubs::resource_kind::ResourceKindSpec;

  #[ntex::test]
  async fn basic() {
    const RESOURCE_KIND_NAME: &str = "test.io/client-test";
    const RESOURCE_KIND_VERSION: &str = "v1";
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    });
    let resource_kind = ResourceKindPartial {
      name: RESOURCE_KIND_NAME.to_owned(),
      version: RESOURCE_KIND_VERSION.to_owned(),
      metadata: None,
      data: ResourceKindSpec {
        schema: None,
        url: Some("unix:///run/nanocl/proxy.sock".to_owned()),
      },
    };
    let resource_kind =
      client.create_resource_kind(&resource_kind).await.unwrap();
    assert_eq!(resource_kind.name, RESOURCE_KIND_NAME);
    let resource_kind = client
      .inspect_resource_kind(RESOURCE_KIND_NAME)
      .await
      .unwrap();
    assert_eq!(resource_kind.name, RESOURCE_KIND_NAME);
    let _ = client.list_resource_kind(None).await.unwrap();
    let _ = client
      .inspect_resource_kind_version(RESOURCE_KIND_NAME, RESOURCE_KIND_VERSION)
      .await
      .unwrap();
    client
      .delete_resource_kind(RESOURCE_KIND_NAME)
      .await
      .unwrap();
  }
}
