use clap::Parser;
use tabled::Tabled;
use serde::{Serialize, Deserialize};

use super::client::Nanocld;
use super::error::{NanocldError, is_api_error};

#[derive(Tabled, Serialize, Deserialize)]
pub struct NamespaceItem {
  pub name: String,
}

#[derive(Debug, Parser)]
#[clap(name = "nanocl-namespace-create")]
pub struct NamespacePartial {
  /// name of the namespace to create
  pub name: String,
}

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
