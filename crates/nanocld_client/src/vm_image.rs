use std::error::Error;

use ntex::util::Bytes;
use ntex::channel::mpsc;
use futures::Stream;

use nanocl_utils::http_error::HttpError;
use nanocl_utils::http_client_error::HttpClientError;

use nanocl_stubs::vm_image::{VmImage, VmImageCloneStream, VmImageResizePayload};

use crate::NanocldClient;

impl NanocldClient {
  pub async fn import_vm_image<S, E>(
    &self,
    name: &str,
    stream: S,
  ) -> Result<(), HttpClientError>
  where
    S: Stream<Item = Result<Bytes, E>> + Unpin + 'static,
    E: Error + 'static,
  {
    self
      .send_post_stream(
        format!("/{}/vms/images/{name}/import", self.version),
        stream,
        None::<String>,
      )
      .await?;
    Ok(())
  }

  pub async fn list_vm_image(&self) -> Result<Vec<VmImage>, HttpClientError> {
    let res = self
      .send_get(format!("/{}/vms/images", self.version), None::<String>)
      .await?;

    Self::res_json(res).await
  }

  pub async fn delete_vm_image(
    &self,
    name: &str,
  ) -> Result<(), HttpClientError> {
    self
      .send_delete(
        format!("/{}/vms/images/{name}", self.version),
        None::<String>,
      )
      .await?;

    Ok(())
  }

  pub async fn clone_vm_image(
    &self,
    name: &str,
    clone_name: &str,
  ) -> Result<
    mpsc::Receiver<Result<VmImageCloneStream, HttpError>>,
    HttpClientError,
  > {
    let res = self
      .send_post(
        format!("/{}/vms/images/{name}/clone/{clone_name}", self.version),
        None::<String>,
        None::<String>,
      )
      .await?;

    Ok(Self::res_stream(res).await)
  }

  pub async fn resize_vm_image(
    &self,
    name: &str,
    payload: &VmImageResizePayload,
  ) -> Result<VmImage, HttpClientError> {
    let res = self
      .send_post(
        format!("/{}/vms/images/{name}/resize", self.version),
        Some(payload.clone()),
        None::<String>,
      )
      .await?;

    Self::res_json(res).await
  }
}
