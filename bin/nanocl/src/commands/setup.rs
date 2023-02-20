use std::collections::HashMap;
use std::path::Path;

use bollard_next::container::StartContainerOptions;
use bollard_next::service::HostConfig;
use futures::StreamExt;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;

use bollard_next::Docker;
use bollard_next::API_DEFAULT_VERSION;
use bollard_next::container::CreateContainerOptions;
use bollard_next::container::WaitContainerOptions;
use bollard_next::service::ProgressDetail;
use bollard_next::image::CreateImageOptions;

use nanocld_client::stubs::cargo_config::ContainerConfig;
use users::get_group_by_name;

use crate::error::CliError;

use crate::models::{SetupOpts, NanocldArgs};
use crate::utils::math::calculate_percentage;
use crate::utils::network::get_default_ip;

fn update_progress(
  multiprogress: &MultiProgress,
  layers: &mut HashMap<String, ProgressBar>,
  id: &str,
  progress: &ProgressDetail,
) {
  let total: u64 = progress
    .total
    .unwrap_or_default()
    .try_into()
    .unwrap_or_default();
  let current: u64 = progress
    .current
    .unwrap_or_default()
    .try_into()
    .unwrap_or_default();
  if let Some(pg) = layers.get(id) {
    let percent = calculate_percentage(current, total);
    pg.set_position(percent);
  } else {
    let pg = ProgressBar::new(100);
    let style = ProgressStyle::with_template(
      "[{elapsed_precise}] [{bar:20.cyan/blue}] {pos:>7}% {msg}",
    )
    .unwrap()
    .progress_chars("=> ");
    pg.set_style(style);
    multiprogress.add(pg.to_owned());
    let percent = calculate_percentage(current, total);
    pg.set_position(percent);
    layers.insert(id.to_owned(), pg);
  }
}

async fn install_image(
  from_image: &str,
  tag: &str,
  docker_api: &Docker,
) -> Result<(), CliError> {
  let options = Some(CreateImageOptions {
    from_image,
    tag,
    ..Default::default()
  });

  let mut stream = docker_api.create_image(options, None, None);
  let mut layers: HashMap<String, ProgressBar> = HashMap::new();
  let multiprogress = MultiProgress::new();
  multiprogress.set_move_cursor(false);
  while let Some(res) = stream.next().await {
    match res {
      Err(err) => {
        return Err(CliError::Custom {
          msg: format!("Error while pulling image: {err}"),
        });
      }
      Ok(data) => {
        let status = data.status.unwrap_or_default();
        let id = data.id.unwrap_or_default();
        let progress = data.progress_detail.unwrap_or_default();
        match status.as_str() {
          "Pulling fs layer" => {
            update_progress(&multiprogress, &mut layers, &id, &progress);
          }
          "Downloading" => {
            update_progress(&multiprogress, &mut layers, &id, &progress);
          }
          "Download complete" => {
            if let Some(pg) = layers.get(&id) {
              pg.set_position(100);
            }
          }
          "Extracting" => {
            update_progress(&multiprogress, &mut layers, &id, &progress);
          }
          _ => {
            if layers.get(&id).is_none() {
              let _ = multiprogress.println(&status);
            }
          }
        };
        if let Some(pg) = layers.get(&id) {
          pg.set_message(format!("[{}] {}", &id, &status));
        }
      }
    }
  }

  Ok(())
}

async fn install_dependencies(
  docker_api: &Docker,
  version: &str,
) -> Result<(), CliError> {
  println!("Installing dependencies");
  install_image("cockroachdb/cockroach", "v22.2.5", docker_api).await?;
  install_image("nexthat/metrsd", "v0.1.0", docker_api).await?;
  install_image("nexthat/nanocld", version, docker_api).await?;
  println!("Dependencies installed");
  Ok(())
}

fn gen_daemon_args(args: &NanocldArgs) -> Vec<String> {
  let hosts = args
    .hosts
    .clone()
    .iter()
    .map(|host| format!("--hosts {}", host))
    .collect::<Vec<String>>();
  let mut args = vec![
    "--state-dir",
    &args.state_dir,
    "--conf-dir",
    &args.conf_dir,
    "--docker-host",
    &args.docker_host,
    "--gateway",
    &args.gateway,
  ]
  .iter()
  .map(|arg| arg.to_string())
  .collect::<Vec<String>>();
  args.extend(hosts);
  args
}

fn gen_daemon_binds(args: &NanocldArgs) -> Vec<String> {
  let host_binds = args
    .hosts
    .iter()
    .filter(|host| host.starts_with("unix://"))
    .map(|host| {
      let path = host.trim_start_matches("unix://");

      let path = Path::new(path)
        .parent()
        .expect("Unix socket path is invalid");

      format!("{host}:{host}", host = path.display())
    });

  let mut base_bind = vec![
    format!("{state_dir}:{state_dir}", state_dir = args.state_dir),
    format!("{conf_dir}:{conf_dir}", conf_dir = args.conf_dir),
  ];

  base_bind.extend(host_binds);

  if args.docker_host.starts_with("unix://") {
    base_bind.push(format!(
      "{docker_host}:{docker_host}",
      docker_host = args.docker_host.trim_start_matches("unix://")
    ));
  }

  let mut binds = Vec::new();
  for b in base_bind {
    if !binds.contains(&b) {
      binds.push(b);
    }
  }

  binds
}

async fn init_dependencies(
  args: &NanocldArgs,
  docker_api: &Docker,
) -> Result<(), CliError> {
  println!("Initing daemon");
  let base_args = gen_daemon_args(args);
  let mut daemon_args = vec!["--init"]
    .iter()
    .map(|arg| arg.to_string())
    .collect::<Vec<String>>();
  daemon_args.extend(base_args);

  let binds = gen_daemon_binds(args);

  let container = docker_api
    .create_container(
      None::<CreateContainerOptions<String>>,
      ContainerConfig {
        image: Some("nexthat/nanocld:nightly".into()),
        cmd: Some(daemon_args),
        env: Some(vec![format!("NANOCL_GID={}", args.gid)]),
        host_config: Some(HostConfig {
          network_mode: Some("host".into()),
          binds: Some(binds),
          auto_remove: Some(true),
          ..Default::default()
        }),
        ..Default::default()
      },
    )
    .await?;
  docker_api
    .start_container(&container.id, None::<StartContainerOptions<String>>)
    .await?;
  let mut stream = docker_api.wait_container(
    &container.id,
    Some(WaitContainerOptions {
      condition: "removed",
    }),
  );
  while let Some(stream) = stream.next().await {
    match stream {
      Ok(data) => {
        if data.status_code != 0 {
          return Err(CliError::Custom {
            msg: format!(
              "Error while initing nanocld daemon: [{}] {}",
              data.status_code,
              data.error.unwrap_or_default().message.unwrap_or_default()
            ),
          });
        }
      }
      Err(err) => match err {
        bollard_next::errors::Error::DockerContainerWaitError {
          error,
          code,
        } => {
          return Err(CliError::Custom {
            msg: format!(
              "Error while initing nanocld daemon: [{}] {}",
              code, error
            ),
          });
        }
        _ => {
          return Err(CliError::Custom {
            msg: format!("Error while initing nanocld daemon: {}", err),
          });
        }
      },
    }
  }
  Ok(())
}

async fn spawn_daemon(
  args: &NanocldArgs,
  docker_api: &Docker,
) -> Result<(), CliError> {
  println!("Spawning daemon");

  let daemon_args = gen_daemon_args(args);
  let mut labels = HashMap::new();
  labels.insert("io.nanocl.cargo".into(), "system-daemon".into());
  labels.insert("io.nanocl.namespace".into(), "system".into());

  let binds = gen_daemon_binds(args);

  let container = docker_api
    .create_container(
      Some(CreateContainerOptions {
        name: "system-daemon",
        ..Default::default()
      }),
      ContainerConfig {
        image: Some("nexthat/nanocld:nightly".into()),
        entrypoint: Some(vec!["/entrypoint.sh".into()]),
        cmd: Some(daemon_args),
        labels: Some(labels),
        env: Some(vec![format!("NANOCL_GID={}", args.gid)]),
        host_config: Some(HostConfig {
          network_mode: Some("system".into()),
          binds: Some(binds),
          ..Default::default()
        }),
        ..Default::default()
      },
    )
    .await?;

  docker_api
    .start_container(&container.id, None::<StartContainerOptions<String>>)
    .await?;

  println!("Daemon spawned");
  Ok(())
}

fn connect_docker(docker_host: &str) -> Result<Docker, CliError> {
  match &docker_host {
    docker_host if docker_host.starts_with("unix://") => {
      let path = docker_host.trim_start_matches("unix://");
      if !std::path::Path::new(&path).exists() {
        return Err(CliError::Custom {
          msg: format!("Error {docker_host} does not exist"),
        });
      }
      Docker::connect_with_unix(path, 120, API_DEFAULT_VERSION).map_err(|err| {
        CliError::Custom {
          msg: format!("Cannot connect to docker got error: {err}"),
        }
      })
    }
    docker_host if docker_host.starts_with("http://") => {
      Docker::connect_with_http(docker_host, 120, API_DEFAULT_VERSION).map_err(
        |err| CliError::Custom {
          msg: format!("Cannot connect to docker got error: {err}"),
        },
      )
    }
    docker_host if docker_host.starts_with("https://") => {
      Docker::connect_with_http(docker_host, 120, API_DEFAULT_VERSION).map_err(
        |err| CliError::Custom {
          msg: format!("Cannot connect to docker got error: {err}"),
        },
      )
    }
    _ => {
      return Err(CliError::Custom {
        msg: format!("Invalid scheme expected [http,https] got: {docker_host}"),
      });
    }
  }
}

pub async fn exec_setup(options: &SetupOpts) -> Result<(), CliError> {
  println!("Installing nanocl daemon on your system");

  let docker_host = options
    .docker_host
    .as_deref()
    .unwrap_or("unix:///var/run/docker.sock")
    .to_owned();

  let state_dir = options
    .state_dir
    .as_deref()
    .unwrap_or("/var/lib/nanocl")
    .to_owned();

  let conf_dir = options
    .conf_dir
    .as_deref()
    .unwrap_or("/etc/nanocl")
    .to_owned();

  let gateway = match &options.gateway {
    None => {
      let gateway = get_default_ip().map_err(|err| CliError::Custom {
        msg: format!("Cannot find default gateway: {err}"),
      })?;
      println!("Using default gateway: {}", gateway);
      gateway.to_string()
    }
    Some(gateway) => gateway.clone(),
  };

  let group = options.group.as_deref().unwrap_or("nanocl");

  let hosts = options
    .deamon_hosts
    .clone()
    .unwrap_or(vec!["unix:///run/nanocl/nanocl.sock".into()]);

  let gid = get_group_by_name(group).ok_or(CliError::Custom {
    msg: format!(
      "Error cannot find group: {group}\n\
    You can create it with: sudo groupadd {group}\n\
    And be sure to add yourself to it: sudo usermod -aG {group} $USER\n\
    Then update your current session: newgrp {group}\n\
    And try again"
    ),
  })?;

  let args = NanocldArgs {
    docker_host,
    state_dir,
    conf_dir,
    gateway,
    hosts,
    gid: gid.gid(),
  };

  let docker_api = connect_docker(&args.docker_host)?;

  install_dependencies(&docker_api, &options.version).await?;

  init_dependencies(&args, &docker_api).await?;

  spawn_daemon(&args, &docker_api).await?;

  println!("Congratz! Nanocl is now ready to use!");
  Ok(())
}
