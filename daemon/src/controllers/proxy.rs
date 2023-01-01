use std::path::Path;

use bollard::{
  Docker,
  errors::Error as DockerError,
  exec::{CreateExecOptions, StartExecOptions},
};

use nanocl_models::cargo::CargoPartial;
use nanocl_models::cargo_config::CargoConfigPartial;

use crate::{utils, repositories};
use crate::errors::DaemonError;
use crate::models::ArgState;

/// Reload proxy config
/// Since our proxy is a nginx image we reload it running `nginx -s reload` inside the proxy container
///
/// ## Arguments
/// [docker_api](Docker) Docker api reference
pub async fn reload_config(docker_api: &Docker) -> Result<(), DockerError> {
  let container_name = "system-nano-proxy";
  let config = CreateExecOptions {
    cmd: Some(vec!["nginx", "-s", "reload"]),
    attach_stdout: Some(true),
    attach_stderr: Some(true),
    ..Default::default()
  };
  let res = docker_api.create_exec(container_name, config).await?;
  let config = StartExecOptions {
    detach: false,
    ..Default::default()
  };
  docker_api.start_exec(&res.id, Some(config)).await?;

  Ok(())
}

/// Register our proxy controller as a cargo
/// So it will be self managed by the system
///
/// ## Arguments
/// [arg](ArgState) Reference to argument state
pub async fn register(arg: &ArgState) -> Result<(), DaemonError> {
  let key = utils::key::gen_key(&arg.sys_namespace, "proxy");
  if repositories::cargo::find_by_key(key, &arg.pool)
    .await
    .is_ok()
  {
    return Ok(());
  }

  let sites_path = Path::new(&arg.config.state_dir).join("nginx/sites-enabled");
  let stream_path =
    Path::new(&arg.config.state_dir).join("nginx/streams-enabled");
  let log_path = Path::new(&arg.config.state_dir).join("nginx/log");
  let ssl_path = Path::new(&arg.config.state_dir).join("nginx/ssl");
  let sock_path = Path::new(&arg.config.state_dir).join("socket");
  let letsencrypt_path =
    Path::new(&arg.config.state_dir).join("nginx/letsencrypt");
  let binds = Some(vec![
    format!("{}:/opt/nanocl-socket", sock_path.display()),
    format!("{}:/etc/nginx/sites-enabled", sites_path.display()),
    format!("{}:/var/log/nginx", log_path.display()),
    format!("{}:/etc/nginx/ssl", ssl_path.display()),
    format!("{}:/etc/nginx/streams-enabled", stream_path.display()),
    format!("{}:/etc/letsencrypt", letsencrypt_path.display()),
  ]);

  let container = bollard::container::Config::<String> {
    image: Some("nanocl-proxy:0.0.1".into()),
    host_config: Some(bollard::models::HostConfig {
      binds,
      network_mode: Some("host".into()),
      restart_policy: Some(bollard::models::RestartPolicy {
        name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
        ..Default::default()
      }),
      ..Default::default()
    }),
    ..Default::default()
  };

  let config = CargoConfigPartial {
    name: "proxy".into(),
    container,
    ..Default::default()
  };

  let proxy_cargo = CargoPartial {
    name: config.name.to_owned(),
    config,
  };

  repositories::cargo::create(
    arg.sys_namespace.to_owned(),
    proxy_cargo,
    &arg.pool,
  )
  .await?;

  Ok(())
}
