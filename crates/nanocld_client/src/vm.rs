use ntex::{rt, ws, io};

use nanocl_error::io::FromIo;
use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm::{Vm, VmSummary, VmInspect};
use nanocl_stubs::vm_config::{VmConfigPartial, VmConfigUpdate};

use crate::NanocldClient;

impl NanocldClient {
  /// ## Default path for vms
  const VM_PATH: &'static str = "/vms";

  /// ## Create vm
  ///
  /// Create a new virtual machine in the system.
  ///
  /// ## Arguments
  ///
  /// * [vm](VmConfigPartial) - The config for the vm
  /// * [namespace](Option) - The [namespace](str) where belong the vm
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Vm](Vm)
  ///
  pub async fn create_vm(
    &self,
    vm: &VmConfigPartial,
    namespace: Option<&str>,
  ) -> HttpClientResult<Vm> {
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
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Vec](Vec) of [VmSummary](VmSummary)
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
  ) -> HttpClientResult<Vec<VmSummary>> {
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
  ) -> HttpClientResult<()> {
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
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [VmInspect](VmInspect)
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
  ) -> HttpClientResult<VmInspect> {
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
  pub async fn start_vm(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
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
  ) -> HttpClientResult<()> {
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
  pub async fn patch_vm(
    &self,
    name: &str,
    vm: &VmConfigUpdate,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
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
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [WsConnection](ws::WsConnection) of [Base](io::Base)
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
  ) -> HttpClientResult<ws::WsConnection<io::Base>> {
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
