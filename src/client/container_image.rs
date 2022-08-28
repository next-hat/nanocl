use futures::{TryStreamExt, StreamExt};
use ntex::{
  rt,
  channel::mpsc::{self, Receiver},
};

use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

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

  pub async fn inspect_image(
    &self,
    name: &str,
  ) -> Result<ContainerImageInspect, NanocldError> {
    let mut res = self
      .get(format!("/containers/images/{}", name))
      .send()
      .await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let ct_image = res.json::<ContainerImageInspect>().await?;

    Ok(ct_image)
  }
}
