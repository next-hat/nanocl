use nanocl_error::http_client::HttpClientResult;
use nanocl_error::io::{FromIo, IoError};
use ntex::{io, ws};

use nanocl_stubs::generic::{GenericFilterNsp, GenericNspQuery};
use nanocl_stubs::vm::{
  Vm, VmInspect, VmLifecycleOptions, VmResizeOptions, VmResources,
  VmRuntimeStatus, VmSummary,
};
use nanocl_stubs::vm_spec::{VmSpec, VmSpecPartial, VmSpecUpdate};

use crate::NanocldClient;

impl NanocldClient {
  /// ## Default path for vms
  const VM_PATH: &'static str = "/vms";

  fn vm_lifecycle_path(key: &str, action: &str) -> HttpClientResult<String> {
    let key = key
      .parse::<nanocl_stubs::resource_key::ResourceKey>()
      .map_err(|err| IoError::invalid_input("VM key", &err.to_string()))?;
    Ok(format!("{}/{key}/{action}", Self::VM_PATH))
  }

  async fn send_vm_lifecycle(
    &self,
    key: &str,
    action: &str,
    options: &VmLifecycleOptions,
  ) -> HttpClientResult<()> {
    options
      .validate()
      .map_err(|err| IoError::invalid_input("VM lifecycle", &err))?;
    let path = Self::vm_lifecycle_path(key, action)?;
    // Include bounded protocol/cleanup time beyond the guest's grace period.
    // Do not retry: a lost response cannot prove the action did not happen.
    let mut response = self
      .post(&path)
      .await?
      .timeout(ntex::time::Millis::from_secs((options.timeout + 60) as u32))
      .send_json(options)
      .await
      .map_err(|err| self.send_error(err))?;
    let status = response.status();
    crate::error::is_api_error(&mut response, &status).await?;
    Ok(())
  }

  /// Stop a VM, waiting for guest shutdown unless force is explicitly requested.
  /// A timeout never forces power-off; inspect its status before retrying.
  pub async fn stop_vm(
    &self,
    key: &str,
    options: &VmLifecycleOptions,
  ) -> HttpClientResult<()> {
    self.send_vm_lifecycle(key, "stop", options).await
  }

  /// Shut down and start a VM. A shutdown timeout prevents the start operation.
  pub async fn restart_vm(
    &self,
    key: &str,
    options: &VmLifecycleOptions,
  ) -> HttpClientResult<()> {
    self.send_vm_lifecycle(key, "restart", options).await
  }

  /// Query actual QEMU execution state through the configured daemon transport.
  pub async fn vm_status(
    &self,
    key: &str,
  ) -> HttpClientResult<VmRuntimeStatus> {
    let path = Self::vm_lifecycle_path(key, "status")?;
    let response = self.send_get(&path, None::<String>).await?;
    Self::res_json(response).await
  }

  /// Grow running QEMU resources. Errors may follow partially applied changes;
  /// query resources before retrying. No automatic retry is performed.
  pub async fn resize_vm(
    &self,
    key: &str,
    options: &VmResizeOptions,
  ) -> HttpClientResult<VmResources> {
    options
      .validate()
      .map_err(|err| IoError::invalid_input("VM resize", &err))?;
    let path = Self::vm_lifecycle_path(key, "resize")?;
    let mut response = self
      .post(&path)
      .await?
      .timeout(ntex::time::Millis::from_secs(120))
      .send_json(options)
      .await
      .map_err(|err| self.send_error(err))?;
    let status = response.status();
    crate::error::is_api_error(&mut response, &status).await?;
    Self::res_json(response).await
  }

  /// Query QEMU capacity and CPU/memory persistence, independently of guest online state.
  pub async fn vm_resources(&self, key: &str) -> HttpClientResult<VmResources> {
    let path = Self::vm_lifecycle_path(key, "resources")?;
    let response = self.send_get(&path, None::<String>).await?;
    Self::res_json(response).await
  }

  fn vm_console_url(&self, key: &str) -> HttpClientResult<String> {
    let key = key
      .parse::<nanocl_stubs::resource_key::ResourceKey>()
      .map_err(|err| {
        IoError::invalid_input("VM console key", &err.to_string())
      })?;
    Ok(self.gen_url(&format!("{}/{key}/console", Self::VM_PATH)))
  }

  /// Connect to an enabled VM graphical console. Binary frames are raw RFB.
  /// Uses the client's configured Unix, HTTP or HTTPS transport and SSL policy.
  pub async fn console_vm(
    &self,
    key: &str,
  ) -> HttpClientResult<ws::WsConnection<io::Sealed>> {
    let url = self.vm_console_url(key)?;
    #[cfg(not(target_os = "windows"))]
    if let Some(path) = &self.unix_socket {
      let socket_path = path.clone();
      return Ok(
        ws::WsClient::new(&url, ntex::SharedCfg::default())
          .connector(async move |cfg: &ntex::SharedCfg, _| {
            Ok(
              ntex::rt::unix_connect(&socket_path, cfg.clone())
                .await
                .map_err(ntex::connect::ConnectError::from)?,
            )
          })
          .connect()
          .await
          .map_err(|err| err.map_err_context(|| "VM console Unix connection"))?
          .seal(),
      );
    }
    if self.url.starts_with("https://") {
      #[cfg(feature = "openssl")]
      return Ok(
        ws::WsClient::new(&url, ntex::SharedCfg::default())
          .openssl(crate::http_client::tls_connector(self.ssl.as_ref())?)
          .connect()
          .await
          .map_err(|err| err.map_err_context(|| "VM console HTTPS connection"))?
          .seal(),
      );
      #[cfg(not(feature = "openssl"))]
      return Err(
        IoError::invalid_input(
          "VM console",
          "Enable the nanocld_client openssl feature for HTTPS",
        )
        .into(),
      );
    }
    Ok(
      ws::WsClient::new(&url, ntex::SharedCfg::default())
        .connect()
        .await
        .map_err(|err| err.map_err_context(|| "VM console HTTP connection"))?
        .seal(),
    )
  }

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

  /// List specification revisions for the current VM incarnation.
  pub async fn list_history_vm(
    &self,
    key: &str,
  ) -> HttpClientResult<Vec<VmSpec>> {
    let path = Self::vm_lifecycle_path(key, "histories")?;
    let response = self.send_get(&path, None::<String>).await?;
    Self::res_json(response).await
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
  /// Patch a VM and return its accepted operation ID.
  pub async fn patch_vm_operation(
    &self,
    key: &str,
    vm: &VmSpecUpdate,
  ) -> HttpClientResult<uuid::Uuid> {
    let response = self
      .send_patch(
        &format!("{}/{key}", Self::VM_PATH),
        Some(vm),
        None::<String>,
      )
      .await?;
    Self::process_operation_header(
      response
        .headers()
        .get("x-nanocl-operation")
        .map(|value| value.as_bytes()),
    )
  }
}

#[cfg(test)]
mod console_tests {
  use super::*;

  #[test]
  fn vm_lifecycle_paths_require_canonical_keys() {
    for action in [
      "stop",
      "restart",
      "status",
      "resize",
      "resources",
      "histories",
    ] {
      assert_eq!(
        NanocldClient::vm_lifecycle_path("team.database", action).unwrap(),
        format!("/vms/team.database/{action}")
      );
      for key in [
        "database",
        "../vm",
        "global.vm?force=true",
        "global.vm/status",
      ] {
        assert!(NanocldClient::vm_lifecycle_path(key, action).is_err());
      }
    }
  }

  #[test]
  fn vm_console_url_requires_a_canonical_key() {
    let client = NanocldClient::connect_with_unix_default();
    assert_eq!(
      client.vm_console_url("global.desktop").unwrap(),
      "http://localhost/v0.19.0/vms/global.desktop/console"
    );
    for key in ["desktop", "global.vm?other", "../vm", "global.vm/console"] {
      assert!(client.vm_console_url(key).is_err());
    }
  }
}
