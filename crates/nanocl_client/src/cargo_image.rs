use ntex::rt;
use ntex::channel::mpsc;
use ntex::http::StatusCode;
use futures::TryStreamExt;

use nanocl_models::cargo_image::CargoImagePartial;

use crate::error::ApiError;

use super::http_client::NanoclClient;
use super::error::{NanoclClientError, is_api_error};

impl NanoclClient {
  /// ## List all cargo images
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - [Vec](Vec) of [ImageSummary](bollard::models::ImageSummary)
  ///   * [Err](Err) - [NanoclClientError](NanoclClientError) if the request failed
  ///
  /// ## Example
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let images = client.list_cargo_image().await;
  /// ```
  ///
  pub async fn list_cargo_image(
    &self,
  ) -> Result<Vec<bollard::models::ImageSummary>, NanoclClientError> {
    let mut res = self.get(String::from("/cargoes/images")).send().await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let body = res.json::<Vec<bollard::models::ImageSummary>>().await?;

    Ok(body)
  }

  /// ## Create a cargo image
  /// This method will create a cargo image and return a stream of [CreateImageInfo](bollard::models::CreateImageInfo)
  /// that can be used to follow the progress of the image creation.
  /// The stream will be closed when the image creation is done.
  ///
  /// ## Arguments
  /// * [name](str) - The name of the image to create
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - [mpsc::Receiver](mpsc::Receiver) of [CreateImageInfo](bollard::models::CreateImageInfo) as Stream
  ///   * [Err](Err) - [NanoclClientError](NanoclClientError) if the request failed
  ///
  /// ## Example
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let mut stream = client.create_cargo_image("my-image").await;
  /// while let Some(info) = stream.try_next().await {
  ///  println!("{:?}", info);
  /// }
  /// ```
  ///
  pub async fn create_cargo_image(
    &self,
    name: &str,
  ) -> Result<mpsc::Receiver<bollard::models::CreateImageInfo>, NanoclClientError>
  {
    let mut res = self
      .post(String::from("/cargoes/images"))
      .send_json(&CargoImagePartial {
        name: name.to_owned(),
      })
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let name = name.to_owned();
    let (sx, rx) = mpsc::channel::<bollard::models::CreateImageInfo>();
    rt::spawn(async move {
      let mut payload_size = 0;
      let mut payload = String::new();
      let mut stream = res.into_stream();
      while let Some(result) = stream.try_next().await.map_err(| err | ApiError {
        msg: format!("Unable to receive stream data while creating image {name} got error : {err}"),
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
            msg: format!("Unable to parse size while creating image {name} got error : {err}"),
            status: StatusCode::INTERNAL_SERVER_ERROR,
          })?;
        }
        // ensure the data size is the same as the payload size
        if data.len() != payload_size {
          payload = format!("{payload}{data}");
        } else {
          // Otherwise we can convert the data into json and send it
          let json = serde_json::from_str::<bollard::models::CreateImageInfo>(data).map_err(| err | ApiError {
            msg: format!("Unable to parse json while creating image {name} got error : {err}"),
            status: StatusCode::INTERNAL_SERVER_ERROR,
          })?;
          payload_size = 0;
          let _ = sx.send(json);
        }
      }
      sx.close();
      Ok::<(), NanoclClientError>(())
    });

    Ok(rx)
  }

  /// ## Delete a cargo image
  /// This method will delete a cargo image by it's name.
  ///
  /// ## Arguments
  /// * [name](str) - The name of the image to delete
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The image was successfully deleted
  ///   * [Err](Err) - [NanoclClientError](NanoclClientError) if the request failed
  ///
  /// ## Example
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// client.delete_cargo_image("my-image:mylabel").await;
  /// ```
  ///
  pub async fn delete_cargo_image(
    &self,
    name: &str,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .delete(format!("/cargoes/images/{name}"))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  /// ## Inspect a cargo image
  /// This method will inspect a cargo image by it's name.
  ///
  /// ## Arguments
  /// * [name](str) - The name of the image to inspect
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - [ImageInspect](bollard::models::ImageInspect) of the image
  ///   * [Err](Err) - [NanoclClientError](NanoclClientError) if the request failed
  ///
  /// ## Example
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let image = client.inspect_cargo_image("my-image:mylabel").await;
  /// ```
  ///
  pub async fn inspect_cargo_image(
    &self,
    name: &str,
  ) -> Result<bollard::models::ImageInspect, NanoclClientError> {
    let mut res = self.get(format!("/cargoes/images/{name}")).send().await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let ct_image = res.json::<bollard::models::ImageInspect>().await?;

    Ok(ct_image)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::StreamExt;

  #[ntex::test]
  async fn test_basic() {
    const IMAGE: &str = "busybox:1.26.1";
    let client = NanoclClient::connect_with_unix_default().await;

    let mut stream = client.create_cargo_image(IMAGE).await.unwrap();
    while let Some(_info) = stream.next().await {}

    client.list_cargo_image().await.unwrap();
    client.inspect_cargo_image(IMAGE).await.unwrap();
    client.delete_cargo_image(IMAGE).await.unwrap();
  }
}
