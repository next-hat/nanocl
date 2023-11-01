use nanocl_error::io::{IoResult};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::cargo_config::CargoConfigPartial;

use super::cargo_config::{
  add_domain_to_env, add_bind_to_hostconfig, generate_cert,
};
use super::secret::{SecretMetadata, create_or_update_secret};

pub async fn update_proxy_rule_cert(
  client: &NanocldClient,
  cargo_config: CargoConfigPartial,
  state_dir: String,
  secret_key: String,
  domain: String,
) -> IoResult<()> {
  let mut cargo_config = cargo_config.to_owned();

  let certs_folder_bind =
    format!("{}/certmanager/certs/{}:/certs", state_dir, domain);

  cargo_config.container.host_config = add_bind_to_hostconfig(
    cargo_config.container.host_config.to_owned(),
    certs_folder_bind,
  );

  if domain != "self-signed" {
    cargo_config.container.env = add_domain_to_env(
      cargo_config.container.env.to_owned(),
      domain.to_owned(),
    );
  }

  let ssl_config =
    generate_cert(client, &cargo_config, domain.to_owned()).await?;

  let metadata = SecretMetadata {
    cert_manager_issuer: cargo_config.name,
    cert_manager_domain: domain,
  };

  create_or_update_secret(client, secret_key, ssl_config, metadata).await?;

  Ok(())
}
