use clap::Parser;
use tabled::Tabled;
use serde::{Serialize, Deserialize};

use super::{
  client::Nanocld,
  error::{NanocldError, is_api_error},
  models::{PgGenericCount, GenericNamespaceQuery},
};

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct CargoPartial {
  /// Name of the cargo
  pub(crate) name: String,
  /// name of the image
  #[clap(long = "image")]
  pub(crate) image_name: String,
  /// Optional domain to bind to in format ip:domain.com
  #[clap(long)]
  pub(crate) dns_entry: Option<String>,
  #[clap(long)]
  pub(crate) domainname: Option<String>,
  #[clap(long)]
  pub(crate) hostname: Option<String>,
  /// proxy config is an optional string as follow domain_name=your_domain,host_ip=your_host_ip
  // #[clap(long)]
  // pub(crate) proxy_config: Option<CargoProxyConfigPartial>,
  #[clap(long = "-bind")]
  /// Directory or volumes to create
  pub(crate) binds: Option<Vec<String>>,
  /// Environement variable
  #[clap(long = "-env")]
  pub(crate) environnements: Option<Vec<String>>,
}

/// Cargo item is an definition to container create image and start them
/// this structure ensure read and write in database
#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct CargoItem {
  pub(crate) key: String,
  pub(crate) name: String,
  #[serde(rename = "image_name")]
  pub(crate) image: String,
  // #[serde(rename = "network_name")]
  // #[tabled(display_with = "optional_string")]
  // pub(crate) network: Option<String>,
  #[serde(rename = "namespace_name")]
  pub(crate) namespace: String,
}

// Helper for tabled may be needed later
fn _optional_string(s: &Option<String>) -> String {
  match s {
    None => String::from(""),
    Some(s) => s.to_owned(),
  }
}

impl Nanocld {
  pub async fn list_cargo(
    &self,
    namespace: Option<String>,
  ) -> Result<Vec<CargoItem>, NanocldError> {
    let mut res = self
      .get(String::from("/cargoes"))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<CargoItem>>().await?;

    Ok(items)
  }

  pub async fn create_cargo(
    &self,
    item: &CargoPartial,
    namespace: Option<String>,
  ) -> Result<CargoItem, NanocldError> {
    let mut res = self
      .post(String::from("/cargoes"))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send_json(item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<CargoItem>().await?;

    Ok(item)
  }

  pub async fn delete_cargo(
    &self,
    cargo_name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/cargoes/{name}", name = cargo_name))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn count_cargo(
    &self,
    namespace: Option<String>,
  ) -> Result<PgGenericCount, NanocldError> {
    let mut res = self
      .get(String::from("/cargoes/count"))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let count = res.json::<PgGenericCount>().await?;
    Ok(count)
  }

  pub async fn inspect_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<CargoItem, NanocldError> {
    let mut res = self
      .get(format!("/cargoes/{name}/inspect", name = name))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<CargoItem>().await?;

    Ok(item)
  }
}
