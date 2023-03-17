use ntex::{ws, rt};
use ntex::io::Base;
use ntex::ws::WsConnection;

use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm::{Vm, VmSummary, VmInspect};
use nanocl_stubs::vm_config::{VmConfigPartial, VmConfigUpdate};

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

  pub async fn list_vm(
    &self,
    namespace: Option<String>,
  ) -> Result<Vec<VmSummary>, NanocldClientError> {
    let res = self
      .send_get(
        format!("/{}/vms", self.version),
        Some(&GenericNspQuery { namespace }),
      )
      .await?;

    Self::res_json(res).await
  }

  pub async fn delete_vm(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldClientError> {
    self
      .send_delete(
        format!("/{}/vms/{}", self.version, name),
        Some(&GenericNspQuery { namespace }),
      )
      .await?;

    Ok(())
  }

  pub async fn inspect_vm(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<VmInspect, NanocldClientError> {
    let res = self
      .send_get(
        format!("/{}/vms/{}/inspect", self.version, name),
        Some(&GenericNspQuery { namespace }),
      )
      .await?;

    Self::res_json(res).await
  }

  pub async fn start_vm(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldClientError> {
    self
      .send_post(
        format!("/{}/vms/{}/start", self.version, name),
        None::<String>,
        Some(&GenericNspQuery { namespace }),
      )
      .await?;

    Ok(())
  }

  pub async fn stop_vm(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldClientError> {
    self
      .send_post(
        format!("/{}/vms/{}/stop", self.version, name),
        None::<String>,
        Some(&GenericNspQuery { namespace }),
      )
      .await?;

    Ok(())
  }

  pub async fn patch_vm(
    &self,
    name: &str,
    vm: &VmConfigUpdate,
    namespace: Option<String>,
  ) -> Result<(), NanocldClientError> {
    self
      .send_patch(
        format!("/{}/vms/{}", self.version, name),
        Some(vm),
        Some(&GenericNspQuery { namespace }),
      )
      .await?;

    Ok(())
  }

  pub async fn attach_vm(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<WsConnection<Base>, NanocldClientError> {
    let qs = if let Some(namespace) = namespace {
      format!("?namespace={}", namespace)
    } else {
      "".to_string()
    };

    // open websockets connection over http transport
    let con = ws::WsClient::build(format!(
      "http://localhost/{}/vms/{name}/attach{qs}",
      &self.version
    ))
    .connector(ntex::service::fn_service(|_| async {
      Ok::<_, _>(rt::unix_connect("/run/nanocl/nanocl.sock").await?)
    }))
    .finish()?
    .connect()
    .await?;

    Ok(con)
  }
}
