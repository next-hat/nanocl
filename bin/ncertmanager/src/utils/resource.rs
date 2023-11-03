use nanocl_error::io::{IoResult, FromIo};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::proxy::{ResourceProxyRule, ProxyRule, ProxySsl};

use nanocld_client::stubs::resource::Resource;

use crate::manager::NCertManager;

use super::secret::{SecretMetadata, generate_certs};

async fn handle_proxy_rule_update(
  proxy_rule: ProxyRule,
  client: &NanocldClient,
  cert_dir: String,
  state_dir: String,
  issuer_key: &String,
) -> IoResult<()> {
  let ssl = match &proxy_rule {
    ProxyRule::Http(proxy_rule) => proxy_rule.ssl.to_owned(),
    ProxyRule::Stream(proxy_rule) => proxy_rule.ssl.to_owned(),
  };

  if let Some(ProxySsl::Secret(secret_key)) = ssl {
    let secret = client.inspect_secret(&secret_key).await;

    if secret.is_ok() {
      return Ok(());
    }

    let domain = if let ProxyRule::Http(proxy_rule) = proxy_rule {
      proxy_rule.domain.to_owned()
    } else {
      None
    };

    let metadata = SecretMetadata {
      cert_manager_issuer: issuer_key.to_string(),
      cert_manager_domain: domain.unwrap_or("localhost".to_string()),
    };

    generate_certs(client, state_dir, cert_dir, secret_key, metadata).await?
  }
  Ok(())
}

pub async fn update_resource_certs<'a>(
  manager: &NCertManager<'a>,
  resource: Resource,
  issuer_key: &String,
) -> IoResult<()> {
  let proxy_rules =
    serde_json::from_value::<ResourceProxyRule>(resource.data.to_owned())
      .map_err(|err| err.map_err_context(|| "ProxyRule data"))?;

  for proxy_rule in proxy_rules.rules.into_iter() {
    if let Err(err) = handle_proxy_rule_update(
      proxy_rule,
      manager.client,
      manager.cert_dir.to_owned(),
      manager.state_dir.to_owned(),
      issuer_key,
    )
    .await
    {
      log::error!("Can't update resource {}: {}", resource.name, err)
    }
  }

  Ok(())
}
