use std::error::Error;

use ntex::util::{Bytes, Stream};
use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpResult;
use nanocl_error::http_client::HttpClientResult;

use bollard_next::service::{ImageSummary, CreateImageInfo, ImageInspect};
use nanocl_stubs::cargo_image::{CargoImagePartial, ListCargoImagesOptions};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for cargo images
  const CARGO_IMAGE_PATH: &'static str = "/cargoes/images";

  /// List cargo images from the system
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_cargo_image(None).await;
  /// ```
  pub async fn list_cargo_image(
    &self,
    opts: Option<&ListCargoImagesOptions>,
  ) -> HttpClientResult<Vec<ImageSummary>> {
    let res = self.send_get(Self::CARGO_IMAGE_PATH, opts).await?;
    Self::res_json(res).await
  }

  /// This method will create a cargo image and return a stream of [CreateImageInfo](bollard_next::models::CreateImageInfo)
  /// that can be used to follow the progress of the image creation.
  /// The stream will be closed when the image creation is done.
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
  pub async fn create_cargo_image(
    &self,
    name: &str,
  ) -> HttpClientResult<Receiver<HttpResult<CreateImageInfo>>> {
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

  /// Delete a cargo image by it's name.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// client.delete_cargo_image("my-image:mylabel").await;
  /// ```
  pub async fn delete_cargo_image(&self, name: &str) -> HttpClientResult<()> {
    self
      .send_delete(
        &format!("{}/{name}", Self::CARGO_IMAGE_PATH),
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// Return detailed information about a cargo image.
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
  ) -> HttpClientResult<ImageInspect> {
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
  ) -> HttpClientResult<()>
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
