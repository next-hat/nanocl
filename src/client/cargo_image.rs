use ntex::rt;
use ntex::http::StatusCode;
use ntex::channel::mpsc::{self, Receiver};
use futures::TryStreamExt;

use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error, ApiError},
};

impl Nanocld {
  pub async fn list_cargo_image(
    &self,
  ) -> Result<Vec<CargoImageSummary>, NanocldError> {
    let mut res = self.get(String::from("/cargoes/images")).send().await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let body = res.json::<Vec<CargoImageSummary>>().await?;

    Ok(body)
  }

  pub async fn create_cargo_image(
    &self,
    name: &str,
  ) -> Result<Receiver<CreateImageStreamInfo>, NanocldError> {
    let mut res = self
      .post(String::from("/cargoes/images"))
      .send_json(&CargoImagePartial {
        name: name.to_owned(),
      })
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let name = name.to_owned();
    let (tx, rx_body) = mpsc::channel::<CreateImageStreamInfo>();
    rt::spawn(async move {
      let mut stream = res.into_stream();
      let mut payload = String::new();
      let mut payload_size = 0;
      while let Some(result) = stream.try_next().await.map_err(| err | ApiError {
        msg: format!("Unable to receive stream data while creating image {} got error : {}", name, err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })? {
        // Convert result as a string
        let chunk = String::from_utf8(result.to_vec())?;
        // Split on new line first line should be the size and second line the data
        let mut lines = chunk.splitn(2, '\n');
        let size = lines.next().unwrap_or_default();
        let data = lines.next().unwrap_or_default().trim();
        // convert the size to a usize
        if payload_size == 0 {
          payload_size = size.parse::<usize>().map_err(| err | ApiError {
            msg: format!("Unable to parse size while creating image {} got error : {}", name, err),
            status: StatusCode::INTERNAL_SERVER_ERROR,
          })?;
        }
        // ensure the data size is the same as the payload size
        if data.len() != payload_size {
          payload = format!("{}{}", payload, data);
        } else {
          // Otherwise we can convert the data into json and send it
          let json = serde_json::from_str::<CreateImageStreamInfo>(data).map_err(| err | ApiError {
            msg: format!("Unable to parse json while creating image {} got error : {}", name, err),
            status: StatusCode::INTERNAL_SERVER_ERROR,
          })?;
          payload_size = 0;
          let _ = tx.send(json);
        }
      }
      tx.close();
      Ok::<(), NanocldError>(())
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

  pub async fn _deploy_cargo_image(
    &self,
    name: &str,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .post(format!("/cargoes/images/{}/deploy", name))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    Ok(())
  }

  pub async fn inspect_cargo_image(
    &self,
    name: &str,
  ) -> Result<CargoImageInspect, NanocldError> {
    let mut res = self.get(format!("/cargoes/images/{}", name)).send().await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let ct_image = res.json::<CargoImageInspect>().await?;

    Ok(ct_image)
  }
}
