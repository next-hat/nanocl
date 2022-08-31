use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

impl Nanocld {
  pub async fn list_namespace(
    &self,
  ) -> Result<Vec<NamespaceItem>, NanocldError> {
    let mut res = self.get(String::from("/namespaces")).send().await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<NamespaceItem>>().await?;

    Ok(items)
  }

  pub async fn create_namespace(
    &self,
    name: &str,
  ) -> Result<NamespaceItem, NanocldError> {
    let new_item = NamespaceItem { name: name.into() };
    let mut res = self
      .post(String::from("/namespaces"))
      .send_json(&new_item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<NamespaceItem>().await?;

    Ok(item)
  }

  pub async fn inspect_namespace(
    &self,
    name: &str,
  ) -> Result<NamespaceItem, NanocldError> {
    let mut res = self
      .get(format!("/namespaces/{name}/inspect", name = name))
      .send()
      .await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<NamespaceItem>().await?;

    Ok(item)
  }
}
