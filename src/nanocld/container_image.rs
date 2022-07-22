use std::collections::HashMap;

use chrono::prelude::*;

use clap::Parser;
use futures::{TryStreamExt, StreamExt};
use ntex::{
  channel::mpsc::{self, Receiver},
  rt,
};
use tabled::Tabled;
use serde::{Serialize, Deserialize, Deserializer, de::DeserializeOwned};

use super::{
  client::Nanocld,
  error::{NanocldError, is_api_error},
  models::ProgressDetail,
};

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct ContainerImagePartial {
  pub(crate) name: String,
}

fn deserialize_nonoptional_vec<
  'de,
  D: Deserializer<'de>,
  T: DeserializeOwned,
>(
  d: D,
) -> Result<Vec<T>, D::Error> {
  serde::Deserialize::deserialize(d).map(|x: Option<_>| x.unwrap_or_default())
}

fn deserialize_nonoptional_map<
  'de,
  D: Deserializer<'de>,
  T: DeserializeOwned,
>(
  d: D,
) -> Result<HashMap<String, T>, D::Error> {
  serde::Deserialize::deserialize(d).map(|x: Option<_>| x.unwrap_or_default())
}

fn display_sha_id(id: &str) -> String {
  let no_sha = id.replace("sha256:", "");
  let (id, _) = no_sha.split_at(12);
  id.to_string()
}

fn display_timestamp(timestamp: &i64) -> String {
  // Create a NaiveDateTime from the timestamp
  let naive = NaiveDateTime::from_timestamp(*timestamp, 0);

  // Create a normal DateTime from the NaiveDateTime
  let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);

  // Format the datetime how you want
  let newdate = datetime.format("%Y-%m-%d %H:%M:%S");
  newdate.to_string()
}

fn display_repo_tags(repos: &[String]) -> String {
  repos[0].to_string()
}

fn print_size(size: &i64) -> String {
  let result = *size as f64 * 1e-9;
  format!("{:.5} GB", result)
}

#[derive(Debug, Tabled, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContainerImageSummary {
  #[serde(rename = "Id")]
  #[tabled(display_with = "display_sha_id")]
  pub id: String,

  #[serde(rename = "ParentId")]
  #[tabled(skip)]
  pub parent_id: String,

  #[serde(rename = "RepoTags")]
  #[serde(deserialize_with = "deserialize_nonoptional_vec")]
  #[tabled(display_with = "display_repo_tags")]
  pub repo_tags: Vec<String>,

  #[serde(rename = "RepoDigests")]
  #[serde(deserialize_with = "deserialize_nonoptional_vec")]
  #[tabled(skip)]
  pub repo_digests: Vec<String>,

  #[serde(rename = "Created")]
  #[tabled(display_with = "display_timestamp")]
  pub created: i64,

  #[serde(rename = "Size")]
  #[tabled(display_with = "print_size")]
  pub size: i64,

  #[serde(rename = "SharedSize")]
  #[tabled(skip)]
  pub shared_size: i64,

  #[serde(rename = "VirtualSize")]
  #[tabled(skip)]
  pub virtual_size: i64,

  #[serde(rename = "Labels")]
  #[serde(deserialize_with = "deserialize_nonoptional_map")]
  #[tabled(skip)]
  pub labels: HashMap<String, String>,

  #[serde(rename = "Containers")]
  #[tabled(skip)]
  pub containers: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateImageStreamInfo {
  #[serde(rename = "id")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,

  #[serde(rename = "error")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,

  #[serde(rename = "status")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub status: Option<String>,

  #[serde(rename = "progress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub progress: Option<String>,

  #[serde(rename = "progressDetail")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub progress_detail: Option<ProgressDetail>,
}

impl Nanocld {
  pub async fn list_container_image(
    &self,
  ) -> Result<Vec<ContainerImageSummary>, NanocldError> {
    let mut res = self.get(String::from("/containers/images")).send().await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let body = res.json::<Vec<ContainerImageSummary>>().await?;

    Ok(body)
  }

  pub async fn create_container_image(
    &self,
    name: &str,
  ) -> Result<Receiver<CreateImageStreamInfo>, NanocldError> {
    let mut res = self
      .post(String::from("/containers/images"))
      .send_json(&ContainerImagePartial {
        name: name.to_owned(),
      })
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let (tx, rx_body) = mpsc::channel::<CreateImageStreamInfo>();
    rt::spawn(async move {
      let mut stream = res.into_stream();
      while let Some(result) = stream.next().await {
        let result = result.unwrap();
        let result = &String::from_utf8(result.to_vec()).unwrap();
        let json =
          serde_json::from_str::<CreateImageStreamInfo>(result).unwrap();
        let _ = tx.send(json);
      }
      tx.close();
    });

    Ok(rx_body)
  }

  pub async fn remove_container_image(
    &self,
    name: &str,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/containers/images/{}", name))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn deploy_container_image(
    &self,
    name: &str,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .post(format!("/containers/images/{}/deploy", name))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    Ok(())
  }
}
