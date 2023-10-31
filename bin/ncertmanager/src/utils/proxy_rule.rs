use std::fs;

use nanocl_error::io::{IoResult, FromIo};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::cargo::{CargoLogQuery, OutputKind};
use nanocld_client::stubs::cargo_config::CargoConfigPartial;
use nanocld_client::stubs::proxy::{ProxyRule, ProxySsl};

use futures::StreamExt;
use nanocld_client::stubs::secret::SecretPartial;

use super::cargo_config::{add_domain_to_env, add_bind_to_hostconfig};

pub async fn generate_cert(
  client: &NanocldClient,
  cargo_config: &CargoConfigPartial,
  domain: String,
  secret_name: String,
) -> IoResult<()> {
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

  let secret = client
    .create_secret(&SecretPartial {
      key: secret_name,
      kind: "Tls".to_owned(),
      data: serde_json::json!({
        "CertificateKey": priv_key,
        "Certificate": full_chain,
      }),
      immutable: None,
      metadata: Some(serde_json::json!({
        "CertManagerIssuer": cargo_config.name,
        "CertManagerDomain": domain
      })),
    })
    .await
    .map_err(|err| err.map_err_context(|| "Create secret"))?;

  log::info!("Secret: {secret:#?}");

  Ok(())
}

pub async fn update_proxy_rule_cert(
  client: &NanocldClient,
  proxy_rule: &ProxyRule,
  cargo_config: CargoConfigPartial,
  state_dir: String,
) -> IoResult<()> {
  let mut cargo_config = cargo_config.to_owned();

  let domain_option = if let ProxyRule::Http(proxy_rule) = proxy_rule {
    proxy_rule.domain.to_owned()
  } else {
    None
  };

  let domain = domain_option.to_owned().unwrap_or("self_signed".to_owned());

  let certs_folder_bind =
    format!("{}/certmanager/certs/{}:/certs", state_dir, domain);

  cargo_config.container.host_config = add_bind_to_hostconfig(
    cargo_config.container.host_config.to_owned(),
    certs_folder_bind,
  );

  if let Some(domain) = domain_option {
    cargo_config.container.env = add_domain_to_env(
      cargo_config.container.env.to_owned(),
      domain.to_owned(),
    );
  }

  let ssl = match proxy_rule {
    ProxyRule::Http(proxy_rule) => proxy_rule.ssl.to_owned(),
    ProxyRule::Stream(proxy_rule) => proxy_rule.ssl.to_owned(),
  };

  if let Some(ProxySsl::Secret(ssl)) = ssl {
    generate_cert(client, &cargo_config, domain, ssl).await?;
  }

  Ok(())
}
