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
  pub async fn list_cargo_image(
    &self,
  ) -> Result<Vec<bollard::models::ImageSummary>, NanocldError> {
    let mut res = self.get(String::from("/cargoes/images")).send().await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let body = res.json::<Vec<bollard::models::ImageSummary>>().await?;

    Ok(body)
  }

  pub async fn create_cargo_image(
    &self,
    name: &str,
  ) -> Result<Receiver<bollard::models::CreateImageInfo>, NanocldError> {
    let mut res = self
      .post(String::from("/cargoes/images"))
      .send_json(&CargoImagePartial {
        name: name.to_owned(),
      })
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let (tx, rx_body) = mpsc::channel::<bollard::models::CreateImageInfo>();
    rt::spawn(async move {
      let mut stream = res.into_stream();
      while let Some(result) = stream.next().await {
        let Ok(result) = result else {
          eprintln!("Stream unable to receive stream data");
          break;
        };
        let Ok(result) = &String::from_utf8(result.to_vec()) else {
          eprintln!("Error Unable to convert incomming stream to string");
          break;
        };
        let Ok(json) =
          serde_json::from_str::<bollard::models::CreateImageInfo>(result) else {
            eprintln!("Error Unable to convert incomming stream to json");
            break;
          };
        let _ = tx.send(json);
      }
      tx.close();
    });

    Ok(rx_body)
  }

  pub async fn remove_cargo_image(
    &self,
    name: &str,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/cargoes/images/{}", name))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn inspect_cargo_image(
    &self,
    name: &str,
  ) -> Result<bollard::models::ImageInspect, NanocldError> {
    let mut res = self.get(format!("/cargoes/images/{}", name)).send().await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let ct_image = res.json::<bollard::models::ImageInspect>().await?;

    Ok(ct_image)
  }
}
