use serde_json::json;

use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

impl Nanocld {
  pub async fn list_cluster(
    &self,
    namespace: Option<String>,
  ) -> Result<Vec<ClusterItem>, NanocldError> {
    let mut res = self
      .get(String::from("/clusters"))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<ClusterItem>>().await?;

    Ok(items)
  }

  pub async fn create_cluster(
    &self,
    item: &ClusterPartial,
    namespace: Option<String>,
  ) -> Result<ClusterItem, NanocldError> {
    let mut res = self
      .post(String::from("/clusters"))
      .query(&GenericNamespaceQuery { namespace })?
      .send_json(&item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<ClusterItem>().await?;

    Ok(item)
  }

  pub async fn inspect_cluster(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<ClusterItemWithRelation, NanocldError> {
    let mut res = self
      .get(format!("/clusters/{name}/inspect", name = name))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<ClusterItemWithRelation>().await?;

    Ok(item)
  }

  pub async fn delete_cluster(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/clusters/{name}", name = name))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn list_cluster_network(
    &self,
    cluster_name: &str,
    namespace: Option<String>,
  ) -> Result<Vec<ClusterNetworkItem>, NanocldError> {
    let mut res = self
      .get(format!("/clusters/{name}/networks", name = cluster_name))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<ClusterNetworkItem>>().await?;

    Ok(items)
  }

  pub async fn create_cluster_network(
    &self,
    cluster_name: &str,
    item: &ClusterNetworkPartial,
    namespace: Option<String>,
  ) -> Result<ClusterNetworkItem, NanocldError> {
    let mut res = self
      .post(format!("/clusters/{name}/networks", name = cluster_name))
      .query(&GenericNamespaceQuery { namespace })?
      .send_json(item)
      .await
      .map_err(NanocldError::SendRequest)?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<ClusterNetworkItem>().await?;

    Ok(item)
  }

  pub async fn delete_cluster_network(
    &self,
    cluster_name: &str,
    network_name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!(
        "/clusters/{c_name}/networks/{n_name}",
        c_name = cluster_name,
        n_name = network_name
      ))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn inspect_cluster_network(
    &self,
    c_name: &str,
    n_name: &str,
    namespace: Option<String>,
  ) -> Result<ClusterNetworkItem, NanocldError> {
    let mut res = self
      .get(format!(
        "/clusters/{c_name}/networks/{n_name}/inspect",
        c_name = c_name,
        n_name = n_name
      ))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let item = res.json::<ClusterNetworkItem>().await?;
    Ok(item)
  }

  pub async fn create_cluster_var(
    &self,
    c_name: &str,
    item: &ClusterVarPartial,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .post(format!("/clusters/{c_name}/variables", c_name = c_name))
      .query(&GenericNamespaceQuery { namespace })?
      .send_json(&item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn inspect_cluster_var(
    &self,
    c_name: &str,
    v_name: &str,
    namespace: Option<String>,
  ) -> Result<ClusterVarPartial, NanocldError> {
    let mut res = self
      .get(format!(
        "/clusters/{c_name}/variables/{v_name}",
        c_name = c_name,
        v_name = v_name
      ))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<ClusterVarPartial>().await?;

    Ok(item)
  }

  pub async fn delete_cluster_var(
    &self,
    c_name: &str,
    v_name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!(
        "/clusters/{c_name}/variables/{v_name}",
        c_name = c_name,
        v_name = v_name
      ))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn join_cluster_cargo(
    &self,
    c_name: &str,
    item: &ClusterJoinPartial,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .post(format!("/clusters/{c_name}/join", c_name = c_name))
      .query(&GenericNamespaceQuery { namespace })?
      .send_json(item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn start_cluster(
    &self,
    c_name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .post(format!("/clusters/{c_name}/start", c_name = c_name))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn count_cluster(
    &self,
    namespace: Option<String>,
  ) -> Result<PgGenericCount, NanocldError> {
    let mut res = self
      .get(String::from("/clusters/count"))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let count = res.json::<PgGenericCount>().await?;

    Ok(count)
  }

  pub async fn count_cluster_network_by_nsp(
    &self,
    namespace: Option<String>,
  ) -> Result<PgGenericCount, NanocldError> {
    let mut res = self
      .get(String::from("/networks/count"))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let count = res.json::<PgGenericCount>().await?;

    Ok(count)
  }

  pub async fn link_proxy_template_to_cluster(
    &self,
    cl_name: &str,
    nt_name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .post(format!("/clusters/{}/proxy/templates", cl_name))
      .query(&GenericNamespaceQuery { namespace })?
      .send_json(&json!({
        "name": nt_name.to_owned(),
      }))
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn unlink_proxy_template_to_cluster(
    &self,
    cl_name: &str,
    nt_name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/clusters/{}/proxy/templates/{}", cl_name, nt_name))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }
}
