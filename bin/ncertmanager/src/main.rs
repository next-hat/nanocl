use clap::Parser;

use nanocl_error::io::{FromIo, IoResult};
use nanocl_utils::logger;
use nanocld_client::NanocldClient;

mod cli;
mod event;
mod version;
mod utils;
mod manager;
mod test;

const CERT_MANAGER_CERT_DIR: &str = "/var/certmanager/certs/";

#[ntex::main]
async fn main() -> IoResult<()> {
  let cli = cli::Cli::parse();
  logger::enable_logger("ncertmanager");
  log::info!("ncertmanager_{}_{}", version::ARCH, version::CHANNEL);
  log::info!("v{}:{}", version::VERSION, version::COMMIT_ID);
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
  }

  event::init_loop(
    &client,
    CERT_MANAGER_CERT_DIR.to_string(),
    cli
      .renew_interval
      .parse()
      .map_err(|err: std::num::ParseIntError| {
        err.map_err_context(|| "Invalid renew interval")
      })?,
  )
  .await?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use futures::StreamExt;
  use nanocld_client::stubs::proxy::ProxySslConfig;

  use crate::{test::tests::gen_default_test_client, utils::secret::SecretMetadata};

  #[ntex::test]
  async fn basic_test() {
    let client = gen_default_test_client().await;
    let state: &str = include_str!("../tests/basic/Statefile.yml");
    let yaml: serde_yaml::Value = serde_yaml::from_str(state).unwrap();
    let json: serde_json::Value = serde_json::to_value(&yaml).unwrap();
    let mut res = client.apply_state(&json).await.unwrap();

    while res.next().await.is_some() {}

    let res = client.wait_cargo("mock-certs", None).await;

    if res.is_ok() {
      let mut res = res.unwrap();
      while res.next().await.is_some() {}
    }

    let secret = client
      .inspect_secret("deploy-example.com@cert-manager")
      .await
      .unwrap();

    let metadata =
      serde_json::from_value::<SecretMetadata>(secret.metadata.unwrap())
        .unwrap();
    let data =
      serde_json::from_value::<ProxySslConfig>(secret.data.clone()).unwrap();

    assert_eq!(
      data.certificate,
      include_str!("../tests/basic/mock/cert.crt")
    );

    assert_eq!(
      data.certificate_key,
      include_str!("../tests/basic/mock/privkey.key")
    );

    assert_eq!(metadata.cert_manager_domain, "deploy-example.com");

    assert_eq!(metadata.cert_manager_issuer, "mock-certs");
  }

  #[ntex::test]
  async fn openssl_test() {
    let client = gen_default_test_client().await;
    let state: &str = include_str!("../tests/openssl/Statefile.yml");
    let yaml: serde_yaml::Value = serde_yaml::from_str(state).unwrap();
    let json: serde_json::Value = serde_json::to_value(&yaml).unwrap();
    let mut res = client.apply_state(&json).await.unwrap();

    while res.next().await.is_some() {}

    let res = client.wait_cargo("openssl", None).await;

    if res.is_ok() {
      let mut res = res.unwrap();
      while res.next().await.is_some() {}
    }

    let secret = client
      .inspect_secret("deploy-example-2.com@cert-manager")
      .await
      .unwrap();

    let metadata =
      serde_json::from_value::<SecretMetadata>(secret.metadata.unwrap())
        .unwrap();
    let data =
      serde_json::from_value::<ProxySslConfig>(secret.data.clone()).unwrap();

    assert!(!data.certificate_key.is_empty());
    assert!(!data.certificate.is_empty());

    assert_eq!(metadata.cert_manager_domain, "deploy-example-2.com");

    assert_eq!(metadata.cert_manager_issuer, "openssl");

    let mut res = client.remove_state(&json).await.unwrap();

    while res.next().await.is_some() {}
  }
}
