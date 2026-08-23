use nanocl_error::http_client::HttpClientResult;

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

  /// List existing VMs, optionally filtering by namespace.
  ///
  /// An omitted, empty, or whitespace-only namespace returns VMs from all
  /// namespaces.
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

  /// Delete a VM by its canonical key.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_vm("global.my-vm").await;
  /// ```
  pub async fn delete_vm(&self, key: &str) -> HttpClientResult<()> {
    self
      .send_delete(&format!("{}/{key}", Self::VM_PATH), None::<String>)
      .await?;
    Ok(())
  }

  /// Inspect a VM by its canonical key.
  /// And get detailed information about it
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.inspect_vm("global.my-vm").await;
  /// ```
  pub async fn inspect_vm(&self, key: &str) -> HttpClientResult<VmInspect> {
    let res = self
      .send_get(&format!("{}/{key}/inspect", Self::VM_PATH), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// Patch a VM by its canonical key.
  pub async fn patch_vm(
    &self,
    key: &str,
    vm: &VmSpecUpdate,
  ) -> HttpClientResult<()> {
    self
      .send_patch(
        &format!("{}/{key}", Self::VM_PATH),
        Some(vm),
        None::<String>,
      )
      .await?;
    Ok(())
  }
}
