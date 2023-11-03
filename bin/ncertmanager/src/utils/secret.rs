use nanocl_error::{
  io::{IoResult, FromIo, IoError},
  http_client::HttpClientError,
};
use nanocld_client::{
  NanocldClient,
  stubs::{
    secret::{Secret, SecretUpdate, SecretPartial},
    proxy::ProxySslConfig,
  },
};
use ntex::http;
use openssl::asn1::Asn1Time;
use serde::{Serialize, Deserialize};

use super::{
  cargo_config::{get_cargo_config, generate_cert, bind_host_infos},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SecretMetadata {
  pub cert_manager_issuer: String,
  pub cert_manager_domain: String,
}

pub async fn create_or_update_secret(
  client: &NanocldClient,
  secret_key: String,
  ssl_config: ProxySslConfig,
  metadata: SecretMetadata,
) -> IoResult<Secret> {
  let secret = client.inspect_secret(&secret_key).await;

  log::debug!("Renew secret {}", secret_key);

  match secret {
    Ok(_) => Ok(
      client
        .patch_secret(
          &secret_key,
          &SecretUpdate {
            data: serde_json::json!(ssl_config),
            metadata: Some(serde_json::json!(metadata)),
          },
        )
        .await
        .map_err(|err| err.map_err_context(|| "Patch secret"))?,
    ),
    Err(HttpClientError::HttpError(err))
      if err.status == http::StatusCode::NOT_FOUND =>
    {
      Ok(
        client
          .create_secret(&SecretPartial {
            key: secret_key,
            kind: "Tls".to_owned(),
            data: serde_json::json!(ssl_config),
            metadata: Some(serde_json::json!(metadata)),
            immutable: None,
          })
          .await
          .map_err(|err| err.map_err_context(|| "Create secret"))?,
      )
    }
    Err(_) => {
      Ok(secret.map_err(|err| err.map_err_context(|| "Inspect secret"))?)
    }
  }
}

pub fn get_expiry_time(secret: &Secret) -> IoResult<u64> {
  let ssl_config =
    serde_json::from_value::<ProxySslConfig>(secret.data.clone())
      .map_err(|err| err.map_err_context(|| "Parse ssl config"))?;

  let cert = openssl::x509::X509::from_pem(ssl_config.certificate.as_bytes())
    .map_err(|err| err.map_err_context(|| "Read certificate"))?;

  let expiry_time = Asn1Time::from_unix(0)
    .map_err(|err| err.map_err_context(|| "Can't get unix time"))?
    .diff(cert.not_after())
    .map_err(|err| err.map_err_context(|| "Can't compare time"))?;

  let expiry = expiry_time.days as u64 * 86400 + expiry_time.secs as u64;

  Ok(expiry)
}

pub async fn generate_certs(
  client: &NanocldClient,
  state_dir: String,
  cert_dir: String,
  secret_key: String,
  metadata: SecretMetadata,
) -> IoResult<()> {
  let mut cargo_config =
    get_cargo_config(client, &metadata.cert_manager_issuer).await?;

  bind_host_infos(
    &mut cargo_config,
    state_dir,
    metadata.cert_manager_domain.to_owned(),
  );

  log::debug!("Run Cargo Config {}", cargo_config.name);

  let ssl_config = generate_cert(
    client,
    &cargo_config,
    cert_dir,
    metadata.cert_manager_domain.to_owned(),
  )
  .await?;

  create_or_update_secret(client, secret_key, ssl_config, metadata).await?;

  Ok(())
}

pub async fn update_secret_cert(
  client: &NanocldClient,
  secret_key: String,
  cert_dir: String,
  state_dir: String,
) -> IoResult<()> {
  let secret = client.inspect_secret(&secret_key).await?;

  match secret.metadata {
    Some(metadata) => {
      let metadata = serde_json::from_value::<SecretMetadata>(metadata)
        .map_err(|err| err.map_err_context(|| "Deserialize secret metadata"))?;

      generate_certs(client, state_dir, cert_dir, secret_key, metadata).await?;

      Ok(())
    }
    None => Err(IoError::invalid_data("Secret", "metadata is missing")),
  }
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::{proxy::ProxySslConfig, resource::ResourcePartial};

  use crate::{test::tests::gen_default_test_client, utils::secret::SecretMetadata};
  use super::generate_certs;

  #[ntex::test]
  async fn cert_generation() {
    let client = gen_default_test_client().await;
    let infos = client.info().await.unwrap();
    let state_dir = infos.config.state_dir;
    let cert_dir = format!("{}/certmanager/certs", &state_dir);

    let basic_state: &str = include_str!("../../tests/basic/Statefile.yml");
    let basic_yaml: serde_yaml::Value =
      serde_yaml::from_str(basic_state).unwrap();
    let resource_basic_config = serde_yaml::from_value::<ResourcePartial>(
      basic_yaml["Resources"][0].clone(),
    )
    .unwrap();

    let secret = client.inspect_secret("secret_key").await;
    assert!(secret.is_err());

    client
      .create_resource(&resource_basic_config)
      .await
      .unwrap();

    generate_certs(
      &client,
      state_dir.to_owned(),
      cert_dir.to_owned(),
      "secret_key".to_string(),
      SecretMetadata {
        cert_manager_domain: "toto".to_string(),
        cert_manager_issuer: "mock-certs-issuer".to_string(),
      },
    )
    .await
    .unwrap();

    let secret = client.inspect_secret("secret_key").await.unwrap();
    let metadata =
      serde_json::from_value::<SecretMetadata>(secret.metadata.unwrap())
        .unwrap();
    let data =
      serde_json::from_value::<ProxySslConfig>(secret.data.clone()).unwrap();

    assert_eq!(
      data.certificate,
      include_str!("../../tests/basic/mock/cert.crt")
    );
    assert_eq!(
      data.certificate_key,
      include_str!("../../tests/basic/mock/privkey.key")
    );
    assert_eq!(metadata.cert_manager_domain, "toto");
    assert_eq!(metadata.cert_manager_issuer, "mock-certs-issuer");

    client.delete_resource("mock-certs-issuer").await.unwrap();

    let openssl_state: &str = include_str!("../../tests/openssl/Statefile.yml");
    let openssl_yaml: serde_yaml::Value =
      serde_yaml::from_str(openssl_state).unwrap();
    let resource_openssl_config = serde_yaml::from_value::<ResourcePartial>(
      openssl_yaml["Resources"][0].clone(),
    )
    .unwrap();

    client
      .create_resource(&resource_openssl_config)
      .await
      .unwrap();

    generate_certs(
      &client,
      state_dir,
      cert_dir,
      "secret_key".to_string(),
      SecretMetadata {
        cert_manager_domain: "tata".to_string(),
        cert_manager_issuer: "openssl-issuer".to_string(),
      },
    )
    .await
    .unwrap();
    let secret = client.inspect_secret("secret_key").await.unwrap();

    let metadata =
      serde_json::from_value::<SecretMetadata>(secret.metadata.unwrap())
        .unwrap();
    let data =
      serde_json::from_value::<ProxySslConfig>(secret.data.clone()).unwrap();

    assert_ne!(
      data.certificate,
      include_str!("../../tests/basic/mock/cert.crt")
    );
    assert_ne!(
      data.certificate_key,
      include_str!("../../tests/basic/mock/privkey.key")
    );
    assert_eq!(metadata.cert_manager_domain, "tata");
    assert_eq!(metadata.cert_manager_issuer, "openssl-issuer");

    client.delete_secret("secret_key").await.unwrap();
    client.delete_resource("openssl-issuer").await.unwrap();
  }
}
