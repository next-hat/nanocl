use nanocl_error::http_client::HttpClientResult;
use nanocl_error::io::{FromIo, IoError};
use ntex::{io, ws};

use nanocl_stubs::generic::{GenericFilterNsp, GenericNspQuery};
use nanocl_stubs::vm::{
  Vm, VmInspect, VmLifecycleOptions, VmRuntimeStatus, VmSummary,
};
use nanocl_stubs::vm_spec::{VmSpecPartial, VmSpecUpdate};

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

  /// Query actual QEMU execution state through the trusted operator transport.
  pub async fn vm_status(
    &self,
    key: &str,
  ) -> HttpClientResult<VmRuntimeStatus> {
    let path = Self::vm_lifecycle_path(key, "status")?;
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
  /// Requires the daemon Unix socket or HTTPS with a verified client certificate.
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
    if !self.url.starts_with("https://") {
      return Err(
        IoError::invalid_input(
          "VM console",
          "Remote console connections require mutual TLS",
        )
        .into(),
      );
    }
    #[cfg(feature = "openssl")]
    {
      let ssl = self.ssl.as_ref().ok_or_else(|| {
        IoError::invalid_input(
          "VM console",
          "Mutual TLS credentials are required",
        )
      })?;
      let connector = console_tls_connector(ssl)?;
      Ok(
        ws::WsClient::new(&url, ntex::SharedCfg::default())
          .openssl(connector)
          .connect()
          .await
          .map_err(|err| {
            err.map_err_context(|| "VM console mutual TLS connection")
          })?
          .seal(),
      )
    }
    #[cfg(not(feature = "openssl"))]
    Err(
      IoError::invalid_input(
        "VM console",
        "Enable the nanocld_client openssl feature for mutual TLS",
      )
      .into(),
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

#[cfg(feature = "openssl")]
fn console_tls_connector(
  ssl: &nanocl_stubs::system::SslConfig,
) -> nanocl_error::io::IoResult<openssl::ssl::SslConnector> {
  use openssl::{
    pkey::PKey,
    ssl::{SslConnector, SslMethod, SslVerifyMode},
    x509::X509,
  };
  if !ssl.verify {
    return Err(IoError::invalid_input(
      "VM console",
      "TLS server verification must be enabled",
    ));
  }
  let required = |value: &Option<String>| {
    value.clone().ok_or_else(|| {
      IoError::invalid_input(
        "VM console",
        "CA, client certificate, and private key are required",
      )
    })
  };
  let ca = required(&ssl.cert_ca)?;
  let cert = required(&ssl.cert)?;
  let key = required(&ssl.cert_key)?;
  let mut builder = SslConnector::builder(SslMethod::tls())
    .map_err(|err| IoError::other("VM console TLS", &err.to_string()))?;
  builder.set_verify(SslVerifyMode::PEER);
  for cert in X509::stack_from_pem(ca.as_bytes())
    .map_err(|err| IoError::other("VM console CA", &err.to_string()))?
  {
    builder
      .cert_store_mut()
      .add_cert(cert)
      .map_err(|err| IoError::other("VM console CA", &err.to_string()))?;
  }
  let cert = X509::from_pem(cert.as_bytes()).map_err(|err| {
    IoError::other("VM console client certificate", &err.to_string())
  })?;
  let key = PKey::private_key_from_pem(key.as_bytes()).map_err(|err| {
    IoError::other("VM console private key", &err.to_string())
  })?;
  builder.set_certificate(&cert).map_err(|err| {
    IoError::other("VM console client certificate", &err.to_string())
  })?;
  builder.set_private_key(&key).map_err(|err| {
    IoError::other("VM console private key", &err.to_string())
  })?;
  builder.check_private_key().map_err(|err| {
    IoError::other("VM console private key", &err.to_string())
  })?;
  Ok(builder.build())
}

#[cfg(test)]
mod console_tests {
  use super::*;

  #[test]
  fn vm_lifecycle_paths_require_canonical_keys() {
    for action in ["stop", "restart", "status"] {
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
