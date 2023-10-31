use nanocl_error::io::{IoResult, FromIo};
use nanocld_client::stubs::proxy::ResourceProxyRule;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::resource::Resource;

use crate::event::ProxyRuleCertManagerMetadata;
use crate::utils::proxy_rule::update_proxy_rule_cert;
use crate::utils::cargo_config::get_cargo_config;

fn get_resource_metadata(
  resource: &Resource,
) -> Option<ProxyRuleCertManagerMetadata> {
  if resource.kind.as_str() != "ProxyRule" {
    log::info!("Non certmanagerissuer resource");
    return None;
  }
  match &resource.metadata {
    Some(metadata) => {
      let metadata = serde_json::from_value::<ProxyRuleCertManagerMetadata>(
        metadata.clone(),
      );

      match metadata {
        Ok(metadata) => Some(metadata),
        Err(error) => {
          log::warn!("Invalid metadatas: {error}");
          None
        }
      }
    }
    None => None,
  }
}

pub async fn update_resource_certs(
  client: &NanocldClient,
  resource: &Resource,
) -> IoResult<()> {
  let metadata = get_resource_metadata(resource);
  match metadata {
    Some(metadata) => {
      let issuer_key: Option<String> = metadata.cert_manager_issuer;
      match issuer_key {
        Some(issuer_key) => {
          let proxy_rules = serde_json::from_value::<ResourceProxyRule>(
            resource.data.to_owned(),
          )
          .map_err(|err| err.map_err_context(|| "ProxyRule data"))?;

          // TODO: what about multiple rules for same domain
          let cargo_config = get_cargo_config(client, issuer_key).await?;
          let infos = client
            .info()
            .await
            .map_err(|err| err.map_err_context(|| "Infos"))?;

          for proxy_rule in proxy_rules.rules.into_iter() {
            update_proxy_rule_cert(
              client,
              &proxy_rule,
              cargo_config.to_owned(),
              infos.config.state_dir.to_owned(),
            )
            .await?
          }

          Ok(())
        }
        None => Ok(()),
      }
    }
    None => Ok(()),
  }
}
