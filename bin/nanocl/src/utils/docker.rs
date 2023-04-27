use std::collections::HashMap;

use futures::StreamExt;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;

use bollard_next::Docker;
use bollard_next::API_DEFAULT_VERSION;
use bollard_next::image::CreateImageOptions;
use bollard_next::container::CreateContainerOptions;
use bollard_next::service::{
  HostConfig, ProgressDetail, RestartPolicy, RestartPolicyNameEnum,
  ContainerCreateResponse,
};

use nanocld_client::stubs::cargo_config::CargoConfigPartial;
use nanocld_client::stubs::cargo_config::Config as ContainerConfig;

use crate::error::CliError;

use crate::utils::math::calculate_percentage;

/// Update progress bar for install image
fn update_image_progress(
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

/// Install image directly with docker
pub async fn install_image(
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
            update_image_progress(&multiprogress, &mut layers, &id, &progress);
          }
          "Downloading" => {
            update_image_progress(&multiprogress, &mut layers, &id, &progress);
          }
          "Download complete" => {
            if let Some(pg) = layers.get(&id) {
              pg.set_position(100);
            }
          }
          "Extracting" => {
            update_image_progress(&multiprogress, &mut layers, &id, &progress);
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

/// Generate a docker client from the docker host
pub fn connect(docker_host: &str) -> Result<Docker, CliError> {
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
    _ => Err(CliError::Custom {
      msg: format!("Invalid scheme expected [http,https] got: {docker_host}"),
    }),
  }
}

pub fn hook_labels(
  key: &str,
  namespace: &str,
  labels: &HashMap<String, String>,
) -> HashMap<String, String> {
  let mut hooked_labels = labels.clone();
  hooked_labels.insert("io.nanocl".into(), "enabled".into());
  hooked_labels.insert("io.nanocl.c".into(), key.to_owned());
  hooked_labels.insert("io.nanocl.n".into(), namespace.to_owned());
  hooked_labels.insert("io.nanocl.cnsp".into(), namespace.to_owned());

  hooked_labels
}

pub async fn create_cargo_container(
  cargo: &CargoConfigPartial,
  namespace: &str,
  docker: &Docker,
) -> Result<ContainerCreateResponse, CliError> {
  let name = &cargo.name;
  let config = &cargo.container;
  let key = format!("{name}.{namespace}");

  let hooked_config = ContainerConfig {
    labels: Some(hook_labels(
      &key,
      namespace,
      &config.labels.clone().unwrap_or_default(),
    )),
    host_config: Some(HostConfig {
      restart_policy: Some(RestartPolicy {
        name: Some(RestartPolicyNameEnum::ALWAYS),
        ..Default::default()
      }),
      ..config.host_config.clone().unwrap_or_default()
    }),
    ..config.clone()
  };

  let container = docker
    .create_container(
      Some(CreateContainerOptions {
        name: format!("{key}.c"),
        platform: None,
      }),
      hooked_config,
    )
    .await?;

  Ok(container)
}
