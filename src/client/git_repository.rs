use ntex::http::StatusCode;
use futures::{TryStreamExt, StreamExt};

use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

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
