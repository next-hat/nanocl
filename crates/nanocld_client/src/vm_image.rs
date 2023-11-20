use std::error::Error;

use ntex::util::Bytes;
use ntex::channel::mpsc;
use futures::Stream;

use nanocl_error::http::HttpError;
use nanocl_error::http_client::HttpClientError;

use nanocl_stubs::vm_image::{VmImage, VmImageCloneStream, VmImageResizePayload};

use crate::NanocldClient;

impl NanocldClient {
  /// ## Default path for vm images
  const VM_IMAGE_PATH: &'static str = "/vms/images";

  /// ## Import a vm image
  ///
  /// This method will import a vm image from a stream of bytes.
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the image to import
  /// * [stream](Stream) - The stream of bytes to import
  ///
  /// ## Return
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - If the operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
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
        &format!("{}/{name}/import", Self::VM_IMAGE_PATH),
        stream,
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// ## List vm images
  ///
  /// List existing vm images in the system.
  ///
  /// ## Return
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vector](Vec) of [vm images](VmImage) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_vm_image().await;
  /// ```
  ///
  pub async fn list_vm_image(&self) -> Result<Vec<VmImage>, HttpClientError> {
    let res = self.send_get(Self::VM_IMAGE_PATH, None::<String>).await?;
    Self::res_json(res).await
  }

  /// ## Delete vm image
  ///
  /// Delete a vm image by it's name
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the vm image to delete
  ///
  /// ## Return
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - If the operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_vm_image("my-image").await;
  /// ```
  ///
  pub async fn delete_vm_image(
    &self,
    name: &str,
  ) -> Result<(), HttpClientError> {
    self
      .send_delete(&format!("{}/{name}", Self::VM_IMAGE_PATH), None::<String>)
      .await?;
    Ok(())
  }

  /// ## Clone vm image
  ///
  /// Clone a vm image by it's name
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the vm image to clone
  /// * [clone_name](str) - The name of the clone
  ///
  /// ## Return
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Stream](mpsc::Receiver) of vm image clone [status](VmImageCloneStream) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.clone_vm_image("my-image", "my-clone").await;
  /// ```
  ///
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
        &format!("{}/{name}/clone/{clone_name}", Self::VM_IMAGE_PATH),
        None::<String>,
        None::<String>,
      )
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Resize vm image
  ///
  /// Resize a vm image by it's name
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the vm image to resize
  /// * [opts](VmImageResizePayload) - The options to resize the vm image
  ///
  /// ## Return
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vm image](VmImage) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.resize_vm_image("my-image", VmImageResizePayload {
  ///   size: 45640,
  ///   shrink: false,
  /// }).await;
  /// ```
  ///
  pub async fn resize_vm_image(
    &self,
    name: &str,
    opts: &VmImageResizePayload,
  ) -> Result<VmImage, HttpClientError> {
    let res = self
      .send_post(
        &format!("{}/{name}/resize", Self::VM_IMAGE_PATH),
        Some(opts.clone()),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }
}
