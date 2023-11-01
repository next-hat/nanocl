use nanocl_error::io::{IoResult, FromIo};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::cargo_config::CargoConfigPartial;
use nanocld_client::stubs::proxy::{ResourceProxyRule, ProxyRule, ProxySsl};

use nanocld_client::stubs::resource::Resource;

use crate::manager::NCertManager;
use crate::utils::proxy_rule::update_proxy_rule_cert;
use crate::utils::cargo_config::get_cargo_config;

async fn handle_proxy_rule_update(
  proxy_rule: ProxyRule,
  client: &NanocldClient,
  cargo_config: CargoConfigPartial,
  state_dir: String,
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

    let domain_option = if let ProxyRule::Http(proxy_rule) = proxy_rule {
      proxy_rule.domain.to_owned()
    } else {
      None
    };

    let domain = domain_option.to_owned().unwrap_or("self_signed".to_owned());

    update_proxy_rule_cert(&client, cargo_config, state_dir, secret_key, domain)
      .await?
  }
  Ok(())
}

pub async fn update_resource_certs(
  client: &NanocldClient,
  resource: Resource,
  issuer_key: String,
) -> IoResult<()> {
  let proxy_rules =
    serde_json::from_value::<ResourceProxyRule>(resource.data.to_owned())
      .map_err(|err| err.map_err_context(|| "ProxyRule data"))?;

  let cargo_config = get_cargo_config(client, issuer_key).await?;

  //TODO dont fetch everytime
  let infos = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Infos"))?;

  for proxy_rule in proxy_rules.rules.into_iter() {
    handle_proxy_rule_update(
      proxy_rule,
      client,
      cargo_config.to_owned(),
      infos.config.state_dir.to_owned(),
    )
    .await?
  }

  Ok(())
}
