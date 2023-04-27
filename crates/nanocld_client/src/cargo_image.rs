use std::error::Error;

use ntex::channel::mpsc;
use ntex::util::{Bytes, Stream};

use nanocl_utils::http_error::HttpError;
use nanocl_utils::http_client_error::HttpClientError;

use nanocl_stubs::cargo_image::{CargoImagePartial, ListCargoImagesOptions};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## List all cargo images
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - [Vec](Vec) of [ImageSummary](bollard_next::models::ImageSummary)
  ///   * [Err](Err) - [HttpClientError](HttpClientError) if the request failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let images = client.list_cargo_image().await;
  /// ```
  ///
  pub async fn list_cargo_image(
    &self,
    opts: Option<ListCargoImagesOptions>,
  ) -> Result<Vec<bollard_next::models::ImageSummary>, HttpClientError> {
    let res = self
      .send_get(format!("/{}/cargoes/images", &self.version), opts)
      .await?;
    Self::res_json(res).await
  }

  /// ## Create a cargo image
  ///
  /// This method will create a cargo image and return a stream of [CreateImageInfo](bollard_next::models::CreateImageInfo)
  /// that can be used to follow the progress of the image creation.
  /// The stream will be closed when the image creation is done.
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the image to create
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - [mpsc::Receiver](mpsc::Receiver) of [CreateImageInfo](bollard_next::models::CreateImageInfo) as Stream
  ///   * [Err](Err) - [HttpClientError](HttpClientError) if the request failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let mut stream = client.create_cargo_image("my-image").await;
  /// while let Some(info) = stream.try_next().await {
  ///  println!("{:?}", info);
  /// }
  /// ```
  ///
  pub async fn create_cargo_image(
    &self,
    name: &str,
  ) -> Result<
    mpsc::Receiver<Result<bollard_next::models::CreateImageInfo, HttpError>>,
    HttpClientError,
  > {
    let res = self
      .send_post(
        format!("/{}/cargoes/images", self.version),
        Some(CargoImagePartial {
          name: name.to_owned(),
        }),
        None::<String>,
      )
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Delete a cargo image
  ///
  /// This method will delete a cargo image by it's name.
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the image to delete
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The image was successfully deleted
  ///   * [Err](Err) - [HttpClientError](HttpClientError) if the request failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// client.delete_cargo_image("my-image:mylabel").await;
  /// ```
  ///
  pub async fn delete_cargo_image(
    &self,
    name: &str,
  ) -> Result<(), HttpClientError> {
    self
      .send_delete(
        format!("/{}/cargoes/images/{name}", self.version),
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// ## Inspect a cargo image
  ///
  /// This method will inspect a cargo image by it's name.
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the image to inspect
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - [ImageInspect](bollard_next::models::ImageInspect) of the image
  ///   * [Err](Err) - [HttpClientError](HttpClientError) if the request failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let image = client.inspect_cargo_image("my-image:mylabel").await;
  /// ```
  ///
  pub async fn inspect_cargo_image(
    &self,
    name: &str,
  ) -> Result<bollard_next::models::ImageInspect, HttpClientError> {
    let res = self
      .send_get(
        format!("/{}/cargoes/images/{name}", self.version),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  pub async fn import_cargo_image_from_tar<S, E>(
    &self,
    stream: S,
  ) -> Result<(), HttpClientError>
  where
    S: Stream<Item = Result<Bytes, E>> + Unpin + 'static,
    E: Error + 'static,
  {
    self
      .send_post_stream(
        format!("/{}/cargoes/images/import", self.version),
        stream,
        None::<String>,
      )
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::StreamExt;

  #[ntex::test]
  async fn basic() {
    const IMAGE: &str = "busybox:1.26.1";
    let client = NanocldClient::connect_with_unix_default();

    let mut stream = client.create_cargo_image(IMAGE).await.unwrap();
    while let Some(_info) = stream.next().await {}

    client.list_cargo_image(None).await.unwrap();
    client.inspect_cargo_image(IMAGE).await.unwrap();
    client.delete_cargo_image(IMAGE).await.unwrap();

    use tokio_util::codec;
    let curr_path = std::env::current_dir().unwrap();
    let filepath =
      std::path::Path::new(&curr_path).join("../../tests/busybox.tar.gz");

    let file = tokio::fs::File::open(&filepath).await.unwrap();

    let byte_stream = codec::FramedRead::new(file, codec::BytesCodec::new())
      .map(|r| {
        let bytes = ntex::util::Bytes::from_iter(r?.to_vec());
        Ok::<ntex::util::Bytes, std::io::Error>(bytes)
      });
    client
      .import_cargo_image_from_tar(byte_stream)
      .await
      .unwrap();
  }
}
