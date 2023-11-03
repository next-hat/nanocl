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
  domain: String,
  state_dir: String,
) -> Option<HostConfig> {
  let certs_folder_bind =
    format!("{}/certmanager/certs/{}:/certs", state_dir, domain);

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

pub fn bind_host_infos(
  cargo_config: &mut CargoConfigPartial,
  state_dir: String,
  domain: String,
) {
  cargo_config.container.host_config = add_bind_to_hostconfig(
    cargo_config.container.host_config.to_owned(),
    domain.to_owned(),
    state_dir,
  );

  if domain != "localhost" {
    cargo_config.container.env = add_domain_to_env(
      cargo_config.container.env.to_owned(),
      domain.to_owned(),
    );
  }
}

pub async fn get_cargo_config(
  client: &NanocldClient,
  issuer_key: &str,
) -> IoResult<CargoConfigPartial> {
  let cert_manager_issuer = client
    .inspect_resource(issuer_key)
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
  cert_dir: String,
  domain: String,
) -> IoResult<ProxySslConfig> {
  client
    .create_cargo(cargo_config, None)
    .await
    .map_err(|err| err.map_err_context(|| "Create cargo"))?;

  log::debug!("Starting {}", cargo_config.name);

  client
    .start_cargo(&cargo_config.name, None)
    .await
    .map_err(|err| err.map_err_context(|| "Start cargo"))?;

  let log_query = CargoLogQuery {
    stderr: Some(true),
    stdout: Some(true),
    ..Default::default()
  };

  // let mut log_stream = client
  //   .wait_cargo(&cargo_config.name, None)
  //   .await
  //   .map_err(|err| err.map_err_context(|| "Log cargo"))?;
  let mut log_stream = client
    .logs_cargo(&cargo_config.name, Some(&log_query))
    .await
    .map_err(|err| err.map_err_context(|| "Log cargo"))?;

  while let Some(output) = log_stream.next().await {
    // let output = output?;
    // log::debug!("Waitstream: {output:#?}");
    let output = output.map_err(|err| err.map_err_context(|| "Log stream"))?;

    match output.kind {
      OutputKind::StdErr => {
        eprint!("{}", output.data);
      }
      OutputKind::StdOut => {
        print!("{}", output.data);
      }
      OutputKind::Console => {
        print!("{}", output.data);
      }
      _ => {}
    }
  }

  let cert_key_path = format!("{cert_dir}/{domain}/{domain}.key");
  log::debug!("Reading certificate key in {cert_key_path}");
  let certificate_key = fs::read_to_string(cert_key_path)?;

  let cert_path = format!("{cert_dir}/{domain}/{domain}.crt");
  log::debug!("Reading certificate in {cert_path}");
  let certificate = fs::read_to_string(cert_path)?;

  Ok(ProxySslConfig {
    certificate_key,
    certificate,
    certificate_client: None,
    verify_client: None,
    dh_param: None,
  })
}

#[cfg(test)]
mod tests {
  use futures::StreamExt;

  use nanocld_client::{
    stubs::{
      cargo_config::{HostConfig, CargoConfigPartial, Config},
      proxy::ProxySslConfig,
    },
  };

  use crate::test::tests::gen_default_test_client;
  use super::{
    add_domain_to_env, add_bind_to_hostconfig, bind_host_infos,
    get_cargo_config,
  };

  #[test]
  fn add_domain() {
    let env = add_domain_to_env(None, "domain".to_string());
    debug_assert_eq!(env, Some(vec!["DOMAIN=domain".to_string()]));
    let env = add_domain_to_env(
      Some(vec!["a".to_string(), "b".to_string()]),
      "domain".to_string(),
    );
    debug_assert_eq!(
      env,
      Some(vec![
        "a".to_string(),
        "b".to_string(),
        "DOMAIN=domain".to_string()
      ])
    );
  }

  #[test]
  fn add_bind() {
    let localhost = "localhost".to_string();
    let state_dir = "state_dir".to_string();
    let config =
      add_bind_to_hostconfig(None, localhost.to_owned(), state_dir.to_owned());

    debug_assert_eq!(
      config,
      Some(HostConfig {
        binds: Some(vec![
          "state_dir/certmanager/certs/localhost:/certs".to_string(),
        ]),
        auto_remove: Some(true),
        ..Default::default()
      })
    );

    let config = add_bind_to_hostconfig(
      config,
      "localhost2".to_owned(),
      "state_dir2".to_owned(),
    );

    debug_assert_eq!(
      config,
      Some(HostConfig {
        binds: Some(vec![
          "state_dir/certmanager/certs/localhost:/certs".to_string(),
          "state_dir2/certmanager/certs/localhost2:/certs".to_string(),
        ]),
        auto_remove: Some(true),
        ..Default::default()
      })
    )
  }

  #[test]
  fn bind_infos() {
    let localhost = "localhost".to_string();
    let state_dir = "state_dir".to_string();
    let mut cargo_config = CargoConfigPartial {
      ..Default::default()
    };

    bind_host_infos(&mut cargo_config, state_dir, localhost);

    debug_assert_eq!(
      cargo_config,
      CargoConfigPartial {
        container: Config {
          host_config: Some(HostConfig {
            auto_remove: Some(true),
            binds: Some(vec![
              "state_dir/certmanager/certs/localhost:/certs".to_string()
            ]),
            ..Default::default()
          }),
          ..Default::default()
        },
        ..Default::default()
      }
    );

    bind_host_infos(
      &mut cargo_config,
      "state_dir2".to_string(),
      "localhost2".to_string(),
    );

    debug_assert_eq!(
      cargo_config,
      CargoConfigPartial {
        container: Config {
          env: Some(vec!["DOMAIN=localhost2".to_string()]),
          host_config: Some(HostConfig {
            auto_remove: Some(true),
            binds: Some(vec![
              "state_dir/certmanager/certs/localhost:/certs".to_string(),
              "state_dir2/certmanager/certs/localhost2:/certs".to_string()
            ]),
            ..Default::default()
          }),
          ..Default::default()
        },
        ..Default::default()
      }
    )
  }

  #[ntex::test]
  pub async fn get_config() {
    use crate::utils::cargo_config::generate_cert;

    let client = gen_default_test_client().await;
    let state: &str = include_str!("../../tests/basic/Statefile.yml");
    let yaml: serde_yaml::Value = serde_yaml::from_str(state).unwrap();
    let json: serde_json::Value = serde_json::to_value(&yaml).unwrap();
    let mut res = client.apply_state(&json).await.unwrap();

    while res.next().await.is_some() {}

    let mut config = get_cargo_config(&client, "mock-certs-issuer")
      .await
      .unwrap();
    let file_config = serde_yaml::from_value::<CargoConfigPartial>(
      yaml["Resources"][0]["Data"]["Generate"].clone(),
    )
    .unwrap();

    assert_eq!(config, file_config);

    let infos = client.info().await.unwrap();

    let cert_dir = format!("{}/certmanager/certs", &infos.config.state_dir);
    let toto_cert_dir = format!("{cert_dir}/toto");

    config.container.env = Some(vec!["DOMAIN=toto".to_string()]);

    config.container.host_config = Some(HostConfig {
      binds: Some(vec![format!("{toto_cert_dir}:/certs")]),
      auto_remove: Some(true),
      ..Default::default()
    });

    let ssl_config =
      generate_cert(&client, &config, cert_dir, "toto".to_string())
        .await
        .unwrap();

    let cert: &str = include_str!("../../tests/basic/mock/cert.crt");
    let privkey: &str = include_str!("../../tests/basic/mock/privkey.key");

    assert_eq!(
      ssl_config,
      ProxySslConfig {
        certificate_key: privkey.to_string(),
        certificate: cert.to_string(),
        certificate_client: None,
        verify_client: None,
        dh_param: None,
      }
    );

    let mut res = client.remove_state(&json).await.unwrap();

    while res.next().await.is_some() {}
  }
}
