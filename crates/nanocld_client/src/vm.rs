use ntex::rt;
use ntex::ws;
use ntex::io::Base;
use ntex::ws::WsConnection;

use nanocl_error::io::FromIo;
use nanocl_error::http_client::HttpClientError;

use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm::{Vm, VmSummary, VmInspect};
use nanocl_stubs::vm_config::{VmConfigPartial, VmConfigUpdate};

use crate::NanocldClient;

impl NanocldClient {
  /// ## Default path for vms
  const VM_PATH: &str = "/vms";

  /// ## Create vm
  ///
  /// Create a new virtual machine in the system.
  ///
  /// ## Arguments
  ///
  /// * [vm](VmConfigPartial) - The config for the vm
  /// * [namespace](Option) - The [namespace](str) where belong the vm
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vm](Vm) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if the operation failed
  ///
  pub async fn create_vm(
    &self,
    vm: &VmConfigPartial,
    namespace: Option<&str>,
  ) -> Result<Vm, HttpClientError> {
    let res = self
      .send_post(
        Self::VM_PATH,
        Some(vm),
        Some(&GenericNspQuery::new(namespace)),
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## List vm
  ///
  /// List existing vms
  ///
  /// ## Arguments
  ///
  /// * [namespace](Option) - The [namespace](str) where belong the vms
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vector](Vec) of [vm summary](VmSummary) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_vm(None).await;
  /// ```
  ///
  pub async fn list_vm(
    &self,
    namespace: Option<&str>,
  ) -> Result<Vec<VmSummary>, HttpClientError> {
    let res = self
      .send_get(Self::VM_PATH, Some(&GenericNspQuery::new(namespace)))
      .await?;
    Self::res_json(res).await
  }

  /// ## Delete vm
  ///
  /// Delete a vm by it's name and namespace
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the vm to delete
  /// * [namespace](Option) - The [namespace](str) where belong the vm
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - The operation succeeded
  ///   * [Err](Err) - [Http client error](HttpClientError) if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_vm("my-vm", None).await;
  /// ```
  ///
  pub async fn delete_vm(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> Result<(), HttpClientError> {
    self
      .send_delete(
        &format!("{}/{name}", Self::VM_PATH),
        Some(&GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// ## Inspect vm
  ///
  /// Inspect a vm by it's name and namespace
  /// And get detailed information about it
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the vm to inspect
  /// * [namespace](Option) - The [namespace](str) where belong the vm
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vm inspect](VmInspect) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.inspect_vm("my-vm", None).await;
  /// ```
  ///
  pub async fn inspect_vm(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> Result<VmInspect, HttpClientError> {
    let res = self
      .send_get(
        &format!("{}/{name}/inspect", Self::VM_PATH),
        Some(&GenericNspQuery::new(namespace)),
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Start vm
  ///
  /// Start a vm by it's name and namespace
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the vm to start
  /// * [namespace](Option) - The [namespace](str) where belong the vm
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - If the operation succeeded
  ///   * [Err](Err) - [Http client error](HttpClientError) if the operation failed
  ///
  pub async fn start_vm(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> Result<(), HttpClientError> {
    self
      .send_post(
        &format!("{}/{name}/start", Self::VM_PATH),
        None::<String>,
        Some(&GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// ## Stop vm
  ///
  /// Stop a vm by it's name and namespace
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the vm to stop
  /// * [namespace](Option) - The [namespace](str) where belong the vm
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - If the operation succeeded
  ///   * [Err](Err) - [Http client error](HttpClientError) if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.stop_vm("my-vm", None).await;
  /// ```
  ///
  pub async fn stop_vm(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> Result<(), HttpClientError> {
    self
      .send_post(
        &format!("{}/{name}/stop", Self::VM_PATH),
        None::<String>,
        Some(&GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// ## Patch vm
  ///
  /// Patch a vm by it's name and namespace to update it's config
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the vm to patch
  /// * [vm](VmConfigUpdate) - The config to update the vm
  /// * [namespace](Option) - The [namespace](str) where belong the vm
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - If the operation succeeded
  ///   * [Err](Err) - [Http client error](HttpClientError) if the operation failed
  ///
  pub async fn patch_vm(
    &self,
    name: &str,
    vm: &VmConfigUpdate,
    namespace: Option<&str>,
  ) -> Result<(), HttpClientError> {
    self
      .send_patch(
        &format!("{}/{name}", Self::VM_PATH),
        Some(vm),
        Some(&GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// ## Attach vm
  ///
  /// Attach to a vm by it's name and namespace
  /// and return websocket stream to send input and receive output from the vm tty
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the vm to attach
  /// * [namespace](Option) - The [namespace](str) where belong the vm
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Websocket connection](WsConnection) to the vm tty if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if the operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.attach_vm("my-vm", None).await;
  /// ```
  ///
  pub async fn attach_vm(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> Result<WsConnection<Base>, HttpClientError> {
    let qs = if let Some(namespace) = namespace {
      format!("?Namespace={}", namespace)
    } else {
      "".to_owned()
    };
    let url = format!("{}/{}/vms/{name}/attach{qs}", self.url, &self.version);
    // open websockets connection over http transport
    let con = match &self.unix_socket {
      Some(path) => ws::WsClient::build(&url)
        .connector(ntex::service::fn_service(|_| async move {
          Ok::<_, _>(rt::unix_connect(&path).await?)
        }))
        .finish()
        .map_err(|err| err.map_err_context(|| path))?
        .connect()
        .await
        .map_err(|err| err.map_err_context(|| path))?,
      None => ws::WsClient::build(&url)
        .finish()
        .map_err(|err| err.map_err_context(|| &self.url))?
        .connect()
        .await
        .map_err(|err| err.map_err_context(|| &self.url))?,
    };
    Ok(con)
  }
}
