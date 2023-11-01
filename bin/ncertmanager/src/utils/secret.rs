use nanocl_error::{
  io::{IoResult, FromIo},
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
            immutable: None,
            metadata: Some(serde_json::json!(metadata)),
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

  let cert =
    openssl::x509::X509::from_pem(ssl_config.certificate.as_bytes()).unwrap();

  let expiry_time = Asn1Time::from_unix(0)
    .unwrap()
    .diff(cert.not_after())
    .unwrap();

  let expiry = expiry_time.days as u64 * 86400 + expiry_time.secs as u64;

  Ok(expiry)
}
