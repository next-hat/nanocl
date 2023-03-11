use nanocl_stubs::vm::Vm;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm_config::VmConfigPartial;

use crate::NanocldClient;
use crate::error::NanocldClientError;

impl NanocldClient {
  pub async fn create_vm(
    &self,
    vm: &VmConfigPartial,
    namespace: Option<String>,
  ) -> Result<Vm, NanocldClientError> {
    let res = self
      .send_post(
        format!("/{}/vms", self.version),
        Some(vm),
        Some(&GenericNspQuery { namespace }),
      )
      .await?;

    Self::res_json(res).await
  }
}
