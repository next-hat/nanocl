use ntex::{io, rt, ws};

use nanocl_error::http_client::HttpClientResult;
use nanocl_error::io::FromIo;

use nanocl_stubs::generic::{GenericFilterNsp, GenericNspQuery};
use nanocl_stubs::vm::{Vm, VmInspect, VmSummary};
use nanocl_stubs::vm_spec::{VmSpecPartial, VmSpecUpdate};

use crate::NanocldClient;

impl NanocldClient {
  /// ## Default path for vms
  const VM_PATH: &'static str = "/vms";

  /// Create a new virtual machine in the system.
  pub async fn create_vm(
    &self,
    vm: &VmSpecPartial,
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

  /// List existing vms
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_vm(None).await;
  /// ```
  pub async fn list_vm(
    &self,
    query: Option<&GenericFilterNsp>,
  ) -> HttpClientResult<Vec<VmSummary>> {
    let query = Self::convert_query(query)?;
    let res = self.send_get(Self::VM_PATH, Some(query)).await?;
    Self::res_json(res).await
  }

  /// Delete a vm by it's name and namespace
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_vm("my-vm", None).await;
  /// ```
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

  /// Inspect a vm by it's name and namespace
  /// And get detailed information about it
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.inspect_vm("my-vm", None).await;
  /// ```
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

  /// Patch a vm by it's name and namespace to update it's spec
  pub async fn patch_vm(
    &self,
    name: &str,
    vm: &VmSpecUpdate,
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

  /// Attach to a vm by it's name and namespace
  /// and return websocket stream to send input and receive output from the vm tty
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.attach_vm("my-vm", None).await;
  /// ```
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
    #[cfg(not(target_os = "windows"))]
    {
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
    #[cfg(target_os = "windows")]
    {
      let con = ws::WsClient::build(&url)
        .finish()
        .map_err(|err| err.map_err_context(|| &self.url))?
        .connect()
        .await
        .map_err(|err| err.map_err_context(|| &self.url))?;
      Ok(con)
    }
  }
}
