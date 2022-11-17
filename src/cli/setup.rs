use std::collections::HashMap;

use users::get_group_by_name;
use bollard::container::{
  CreateContainerOptions, Config, StartContainerOptions, WaitContainerOptions,
  RemoveContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::service::{HostConfig, RestartPolicy, RestartPolicyNameEnum};
use futures::StreamExt;
use indicatif::{ProgressStyle, ProgressBar};
use std::default::Default;

use crate::models::SetupArgs;
use crate::utils::cargo_image;
use crate::config::{read_daemon_config_file, DaemonConfig};

use super::errors::CliError;

const DAEMON_VERSION: &str = "0.1.17";

async fn instance_exists(
  name: &str,
  docker_api: &bollard::Docker,
) -> Result<bool, CliError> {
  match docker_api.inspect_container(name, None).await {
    Err(_) => Ok(false),
    Ok(_) => Ok(true),
  }
}

async fn image_exists(
  name: &str,
  docker_api: &bollard::Docker,
) -> Result<bool, CliError> {
  match docker_api.inspect_image(name).await {
    Err(_) => Ok(false),
    Ok(_) => Ok(true),
  }
}

async fn install_store_image(
  docker_api: &bollard::Docker,
) -> Result<(), CliError> {
  let store_image = "cockroachdb/cockroach:v21.2.17";

  if image_exists(store_image, &docker_api).await? {
    return Ok(());
  }

  let options = Some(CreateImageOptions {
    from_image: "cockroachdb/cockroach:v21.2.17",
    ..Default::default()
  });

  let mut stream = docker_api.create_image(options, None, None);
  let style = ProgressStyle::default_spinner();
  let pg = ProgressBar::new(0);
  pg.set_style(style);
  while let Some(chunk) = stream.next().await {
    if let Err(err) = chunk {
      eprintln!("Error while downloading store image: {err}");
      std::process::exit(1);
    }
    pg.tick();
  }
  pg.finish_and_clear();
  Ok(())
}

async fn install_daemon_image(
  docker_api: &bollard::Docker,
) -> Result<(), CliError> {
  let image = format!("nanocl-daemon:{version}", version = DAEMON_VERSION);
  if image_exists(&image, docker_api).await? {
    return Ok(());
  }

  let daemon_image_url = format!("https://github.com/nxthat/nanocld/releases/download/v{version}/nanocl-daemon.{version}.tar.gz", version = DAEMON_VERSION);
  cargo_image::import_tar_from_url(docker_api, &daemon_image_url).await?;
  Ok(())
}

async fn init_daemon(
  config: &DaemonConfig,
  docker_api: &bollard::Docker,
) -> Result<(), CliError> {
  let host_config = HostConfig {
    binds: Some(vec![
      String::from("/run/nanocl:/run/nanocl"),
      String::from("/var/lib/nanocl:/var/lib/nanocl"),
      format!("{}:/run/docker.sock", &config.docker_host),
    ]),
    network_mode: Some(String::from("host")),
    ..Default::default()
  };
  let image = format!("nanocl-daemon:{version}", version = DAEMON_VERSION);
  let gid = get_gid()?;
  let nanocl_gid = format!("NANOCL_GID={gid}");
  let config = Config {
    cmd: Some(vec!["--init"]),
    image: Some(&image),
    env: Some(vec![nanocl_gid.as_ref()]),
    host_config: Some(host_config),
    ..Default::default()
  };

  let options = Some(CreateContainerOptions {
    name: "system-nanocl-daemon",
  });

  let c_res = docker_api.create_container(options, config).await?;

  docker_api
    .start_container(
      "system-nanocl-daemon",
      None::<StartContainerOptions<String>>,
    )
    .await?;

  let options = WaitContainerOptions {
    condition: "next-exit",
  };

  let mut stream = docker_api.wait_container(&c_res.id, Some(options));

  while let Some(_chunk) = stream.next().await {}

  let options = Some(RemoveContainerOptions {
    force: true,
    ..Default::default()
  });

  docker_api.remove_container(&c_res.id, options).await?;

  Ok(())
}

async fn spawn_deamon(
  config: &DaemonConfig,
  docker_api: &bollard::Docker,
) -> Result<(), CliError> {
  let host_config = HostConfig {
    binds: Some(vec![
      String::from("/run/nanocl:/run/nanocl"),
      String::from("/var/lib/nanocl:/var/lib/nanocl"),
      format!("{}:/run/docker.sock", &config.docker_host),
    ]),
    restart_policy: Some(RestartPolicy {
      name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
      maximum_retry_count: None,
    }),
    network_mode: Some(String::from("host")),
    ..Default::default()
  };
  let image = format!("nanocl-daemon:{version}", version = DAEMON_VERSION);
  let mut labels = HashMap::new();
  labels.insert("namespace", "system");
  labels.insert("cluster", "system-nano");
  labels.insert("cargo", "system-daemon");
  let gid = get_gid()?;
  let nanocl_gid = format!("NANOCL_GID={gid}");
  let config = Config {
    image: Some(image.as_ref()),
    labels: Some(labels),
    env: Some(vec![nanocl_gid.as_ref()]),
    host_config: Some(host_config),
    ..Default::default()
  };

  let options = Some(CreateContainerOptions {
    name: "system-nanocl-daemon",
  });

  docker_api.create_container(options, config).await?;

  docker_api
    .start_container(
      "system-nanocl-daemon",
      None::<StartContainerOptions<String>>,
    )
    .await?;
  Ok(())
}

fn get_gid() -> Result<u32, CliError> {
  let group = get_group_by_name("nanocl").ok_or(CliError::Custom {
    msg: String::from("group nanocl must exists"),
  })?;

  let gid = group.gid();

  Ok(gid)
}

pub async fn exec_setup(args: &SetupArgs) -> Result<(), CliError> {
  let config = read_daemon_config_file(&String::from("/etc/nanocl"))?;
  match &args.host {
    // Host is empty perform local installation
    None => {
      // Connect to docker daemon
      let docker_api = bollard::Docker::connect_with_unix(
        &config.docker_host,
        120,
        bollard::API_DEFAULT_VERSION,
      )?;
      install_store_image(&docker_api).await?;
      install_daemon_image(&docker_api).await?;
      if instance_exists("system-nanocl-daemon", &docker_api).await? {
        return Ok(());
      }
      init_daemon(&config, &docker_api).await?;
      spawn_deamon(&config, &docker_api).await?;
    }
    // Host is exists perform remote installation
    Some(_host) => {
      todo!("Remote installation is not available yet.");
    }
  }
  Ok(())
}
