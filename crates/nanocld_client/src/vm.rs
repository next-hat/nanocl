use futures::Stream;
use nanocl_stubs::vm::{Vm, VmSummary, VmInspect};
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::vm_config::VmConfigPartial;
use ntex::connect::Connect;
use ntex::http::Method;
use ntex::http::client::ClientResponse;
use ntex::http::header::{HeaderName, HeaderValue};
use ntex::util::Bytes;
use ntex::{rt, ws};

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

  pub async fn attach_vm<S, E>(
    &self,
    name: &str,
    namespace: Option<String>,
    stream: S,
  ) -> Result<ClientResponse, NanocldClientError>
  where
    S: Stream<Item = Result<Bytes, E>> + Unpin + 'static,
    E: std::error::Error + 'static,
  {
    let url = format!("/{}/vms/{}/attach", self.version, name);

    // open websockets connection over http transport
    // let con = ws::WsClient::build(url)
    //   .connector(
    //     Connect::default()
    //       .connector(ntex::service::fn_service(|_| async {
    //         Ok::<_, _>(rt::unix_connect("/run/nanocl/nanocl.sock").await?)
    //       }))
    //       .timeout(ntex::time::Millis::from_secs(50))
    //       .finish(),
    //   )
    //   .finish()
    //   .unwrap()
    //   .connect()
    //   .await
    //   .unwrap();

    let res = self
      .send_post_stream(
        format!("/{}/vms/{}/attach", self.version, name),
        stream,
        Some(&GenericNspQuery { namespace }),
      )
      .await?;

    println!("res: {:?}", res);

    Ok(res)
  }
}
