use std::error::Error;

use ntex::channel::mpsc;
use ntex::util::{Bytes, Stream};

use nanocl_error::http::HttpError;
use nanocl_error::http_client::HttpClientError;

use nanocl_stubs::cargo_image::{CargoImagePartial, ListCargoImagesOptions};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for cargo images
  const CARGO_IMAGE_PATH: &'static str = "/cargoes/images";

  /// ## List cargo image
  ///
  /// List cargo images from the system
  ///
  /// ## Arguments
  ///
  /// * [opts](Option) - The optional [query](ListCargoImagesOptions)
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vector](Vec) of [image summary](bollard_next::models::ImageSummary) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_cargo_image(None).await;
  /// ```
  ///
  pub async fn list_cargo_image(
    &self,
    opts: Option<&ListCargoImagesOptions>,
  ) -> Result<Vec<bollard_next::models::ImageSummary>, HttpClientError> {
    let res = self.send_get(Self::CARGO_IMAGE_PATH, opts).await?;
    Self::res_json(res).await
  }

  /// ## Create cargo image
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
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Receiver](mpsc::Receiver) of [CreateImageInfo](bollard_next::models::CreateImageInfo) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let mut stream = client.create_cargo_image("my-image").await;
  /// while let Some(info) = stream.try_next().await {
  ///  println!("{info:?}");
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
        Self::CARGO_IMAGE_PATH,
        Some(CargoImagePartial {
          name: name.to_owned(),
        }),
        None::<String>,
      )
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Delete cargo image
  ///
  /// Delete a cargo image by it's name.
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the image to delete
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The image was successfully deleted if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// client.delete_cargo_image("my-image:mylabel").await;
  /// ```
  ///
  pub async fn delete_cargo_image(
    &self,
    name: &str,
  ) -> Result<(), HttpClientError> {
    self
      .send_delete(
        &format!("{}/{name}", Self::CARGO_IMAGE_PATH),
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// ## Inspect cargo image
  ///
  /// Return detailed information about a cargo image.
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the image to inspect
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Image inspect](bollard_next::models::ImageInspect) of the image if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let image = client.inspect_cargo_image("my-image:mylabel").await;
  /// ```
  ///
  pub async fn inspect_cargo_image(
    &self,
    name: &str,
  ) -> Result<bollard_next::models::ImageInspect, HttpClientError> {
    let res = self
      .send_get(
        &format!("{}/{name}", Self::CARGO_IMAGE_PATH),
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
        &format!("{}/import", Self::CARGO_IMAGE_PATH),
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
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
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
