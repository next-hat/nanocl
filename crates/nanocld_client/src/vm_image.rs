use std::error::Error;

use ntex::util::Bytes;
use ntex::http::client::ClientResponse;

use futures::Stream;
use futures::TryStreamExt;
use futures::stream::IntoStream;

use crate::NanocldClient;
use crate::error::NanocldClientError;

impl NanocldClient {
  pub async fn create_vm_image_base_from_tar<S, E>(
    &self,
    name: &str,
    stream: S,
  ) -> Result<IntoStream<ClientResponse>, NanocldClientError>
  where
    S: Stream<Item = Result<Bytes, E>> + Unpin + 'static,
    E: Error + 'static,
  {
    let res = self
      .send_post_stream(
        format!("/{}/vms/images/{name}/base", self.version),
        stream,
        None::<String>,
      )
      .await?;
    let stream = res.into_stream();
    Ok(stream)
  }
}
