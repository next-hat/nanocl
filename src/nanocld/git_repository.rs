use ntex::http::StatusCode;
use tabled::Tabled;
use clap::{Parser, arg_enum};
use serde::{Serialize, Deserialize};
use futures::{TryStreamExt, StreamExt};

use super::client::Nanocld;

use super::error::{NanocldError, is_api_error};
use super::models::ProgressDetail;

arg_enum! {
  #[derive(Debug, Tabled, Serialize, Deserialize)]
  #[serde(rename_all = "lowercase")]
  pub enum GitRepositorySourceType {
    Github,
    Gitlab,
    Local,
  }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ErrorDetail {
  #[serde(rename = "code")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub code: Option<i64>,

  #[serde(rename = "message")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
}

/// Image ID or Digest
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageId {
  #[serde(rename = "ID")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GithubRepositoryBuildStream {
  #[serde(rename = "id")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,

  #[serde(rename = "stream")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stream: Option<String>,

  #[serde(rename = "error")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,

  #[serde(rename = "errorDetail")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error_detail: Option<ErrorDetail>,

  #[serde(rename = "status")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub status: Option<String>,

  #[serde(rename = "progress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub progress: Option<String>,

  #[serde(rename = "progressDetail")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub progress_detail: Option<ProgressDetail>,

  #[serde(rename = "aux")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub aux: Option<ImageId>,
}

#[derive(Tabled, Serialize, Deserialize)]
pub struct GitRepositoryItem {
  pub(crate) name: String,
  pub(crate) url: String,
  pub(crate) default_branch: String,
  pub(crate) source: GitRepositorySourceType,
}

#[derive(Debug, Parser, Serialize)]
pub struct GitRepositoryPartial {
  pub(crate) name: String,
  #[clap(long)]
  pub(crate) url: String,
}

impl Nanocld {
  pub async fn list_git_repository(
    &self,
  ) -> Result<Vec<GitRepositoryItem>, NanocldError> {
    let mut res = self.get(String::from("/git_repositories")).send().await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<GitRepositoryItem>>().await?;

    Ok(items)
  }

  pub async fn create_git_repository(
    &self,
    item: &GitRepositoryPartial,
  ) -> Result<GitRepositoryItem, NanocldError> {
    let mut res = self
      .post(String::from("/git_repositories"))
      .send_json(&item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let body = res.json::<GitRepositoryItem>().await?;

    Ok(body)
  }

  pub async fn build_git_repository<C>(
    &self,
    name: String,
    mut callback: C,
  ) -> Result<(), NanocldError>
  where
    C: FnMut(GithubRepositoryBuildStream),
  {
    let mut res = self
      .post(format!("/git_repositories/{name}/build", name = name))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    if status == StatusCode::NOT_MODIFIED {
      return Ok(());
    }
    let mut stream = res.into_stream();
    while let Some(result) = stream.next().await {
      let result = result.map_err(NanocldError::Payload)?;
      let result = &String::from_utf8(result.to_vec()).unwrap();
      let json =
        serde_json::from_str::<GithubRepositoryBuildStream>(result).unwrap();
      callback(json);
    }

    Ok(())
  }

  pub async fn delete_git_repository(
    &self,
    name: String,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/git_repositories/{name}", name = name))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }
}
