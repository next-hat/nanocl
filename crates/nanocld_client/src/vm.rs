use ntex::rt;
use ntex::ws;
use ntex::io::Base;
use ntex::ws::WsConnection;

use nanocl_utils::io_error::FromIo;
use nanocl_utils::http_client_error::HttpClientError;

use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm::{Vm, VmSummary, VmInspect};
use nanocl_stubs::vm_config::{VmConfigPartial, VmConfigUpdate};

use crate::NanocldClient;

impl NanocldClient {
  pub async fn create_vm(
    &self,
    vm: &VmConfigPartial,
    namespace: Option<String>,
  ) -> Result<Vm, HttpClientError> {
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
  ) -> Result<Vec<VmSummary>, HttpClientError> {
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
  ) -> Result<(), HttpClientError> {
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
  ) -> Result<VmInspect, HttpClientError> {
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
  ) -> Result<(), HttpClientError> {
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
  ) -> Result<(), HttpClientError> {
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
  ) -> Result<(), HttpClientError> {
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
  ) -> Result<WsConnection<Base>, HttpClientError> {
    let qs = if let Some(namespace) = namespace {
      format!("?namespace={}", namespace)
    } else {
      "".to_string()
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
