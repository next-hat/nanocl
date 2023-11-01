use std::fs;

use futures::StreamExt;
use nanocl_error::io::{FromIo, IoResult};
use nanocld_client::stubs::cargo::{CargoLogQuery, OutputKind};
use nanocld_client::stubs::cargo_config::{CargoConfigPartial, HostConfig};
use nanocld_client::stubs::cert_manager::CertManagerIssuer;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::proxy::ProxySslConfig;

pub fn add_domain_to_env(
  env: Option<Vec<String>>,
  domain: String,
) -> Option<Vec<String>> {
  match env {
    Some(env) => {
      let mut env = env.clone();

      env.push(format!("DOMAIN={domain}").to_owned());

      Some(env)
    }
    None => Some(vec![format!("DOMAIN={domain}").to_owned()]),
  }
}

pub fn add_bind_to_hostconfig(
  host_config: Option<HostConfig>,
  certs_folder_bind: String,
) -> Option<HostConfig> {
  match host_config {
    Some(host_config) => {
      let mut host_config = host_config.to_owned();

      host_config.auto_remove = Some(true);

      match &mut host_config.binds {
        Some(binds) => {
          binds.push(certs_folder_bind);
        }
        None => host_config.binds = Some(vec![certs_folder_bind]),
      }
      Some(host_config)
    }
    None => Some(HostConfig {
      binds: Some(vec![certs_folder_bind]),
      auto_remove: Some(true),
      ..Default::default()
    }),
  }
}

pub(crate) async fn get_cargo_config(
  client: &NanocldClient,
  issuer_key: String,
) -> IoResult<CargoConfigPartial> {
  let cert_manager_issuer = client
    .inspect_resource(issuer_key.as_str())
    .await
    .map_err(|err| err.map_err_context(|| "Inspect resource"))?;

  let cargo_config = serde_json::from_value::<CertManagerIssuer>(
    cert_manager_issuer.data.to_owned(),
  )
  .map_err(|err| err.map_err_context(|| "CertManagerIssuer config"))?
  .generate;

  Ok(cargo_config)
}

pub async fn generate_cert(
  client: &NanocldClient,
  cargo_config: &CargoConfigPartial,
  domain: String,
) -> IoResult<ProxySslConfig> {
  client
    .create_cargo(cargo_config, None)
    .await
    .map_err(|err| err.map_err_context(|| "Create cargo"))?;

  client
    .start_cargo(&cargo_config.name, None)
    .await
    .map_err(|err| err.map_err_context(|| "Start cargo"))?;

  let log_query = CargoLogQuery {
    stderr: Some(true),
    ..Default::default()
  };

  let mut log_stream = client
    .logs_cargo(&cargo_config.name, &log_query)
    .await
    .map_err(|err| err.map_err_context(|| "Log cargo"))?;

  while let Some(output) = log_stream.next().await {
    let output = output.map_err(|err| err.map_err_context(|| "Log stream"))?;

    if let OutputKind::StdErr = output.kind {
      eprint!("{}", output.data);
    }
  }

  let priv_key = fs::read_to_string(format!(
    "/var/certmanager/certs/{}/privkey.pem",
    domain
  ))?;

  let full_chain = fs::read_to_string(format!(
    "/var/certmanager/certs/{}/fullchain.pem",
    domain
  ))?;

  Ok(ProxySslConfig {
    certificate_key: priv_key,
    certificate: full_chain,
    certificate_client: None,
    verify_client: None,
    dh_param: None,
  })
}
