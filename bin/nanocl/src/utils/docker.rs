use std::collections::HashMap;

use futures::StreamExt;
use indicatif::{ProgressBar, MultiProgress, ProgressStyle};

use bollard_next::{Docker, API_DEFAULT_VERSION};
use bollard_next::image::CreateImageOptions;
use bollard_next::container::CreateContainerOptions;
use bollard_next::service::{
  HostConfig, ProgressDetail, RestartPolicy, RestartPolicyNameEnum,
  ContainerCreateResponse,
};

use nanocl_error::io::{IoError, FromIo, IoResult};

use nanocld_client::stubs::cargo_spec::{CargoSpecPartial, Config};

use crate::utils::hash;
use crate::utils::math::calculate_percentage;
use crate::models::DockerContextMeta;

/// Update progress bar for install image
fn update_image_progress(
  multi_progress: &MultiProgress,
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
    multi_progress.add(pg.to_owned());
    let percent = calculate_percentage(current, total);
    pg.set_position(percent);
    layers.insert(id.to_owned(), pg);
  }
}

/// Install image directly with docker and output progress
pub async fn install_image(
  from_image: &str,
  tag: &str,
  docker_api: &Docker,
) -> IoResult<()> {
  let options = Some(CreateImageOptions {
    from_image,
    tag,
    ..Default::default()
  });
  let mut stream = docker_api.create_image(options, None, None);
  let mut layers: HashMap<String, ProgressBar> = HashMap::new();
  let multi_progress = MultiProgress::new();
  multi_progress.set_move_cursor(false);
  while let Some(res) = stream.next().await {
    let data = res
      .map_err(|err| err.map_err_context(|| "Install image stream failed"))?;
    let status = data.status.unwrap_or_default();
    let id = data.id.unwrap_or_default();
    let progress = data.progress_detail.unwrap_or_default();
    match status.as_str() {
      "Pulling fs layer" => {
        update_image_progress(&multi_progress, &mut layers, &id, &progress);
      }
      "Downloading" => {
        update_image_progress(&multi_progress, &mut layers, &id, &progress);
      }
      "Download complete" => {
        if let Some(pg) = layers.get(&id) {
          pg.set_position(100);
        }
      }
      "Extracting" => {
        update_image_progress(&multi_progress, &mut layers, &id, &progress);
      }
      _ => {
        if layers.get(&id).is_none() {
          let _ = multi_progress.println(&status);
        }
      }
    };
    if let Some(pg) = layers.get(&id) {
      pg.set_message(format!("[{}] {}", &id, &status));
    }
  }

  Ok(())
}

/// Generate a docker client from the docker host
pub fn connect(docker_host: &str) -> IoResult<Docker> {
  let docker = match &docker_host {
    docker_host if docker_host.starts_with("unix://") => {
      let path = docker_host.trim_start_matches("unix://");
      if !std::path::Path::new(&path).exists() {
        return Err(IoError::not_found(
          "Unix socket file can't be found at",
          path,
        ));
      }
      Docker::connect_with_unix(path, 120, API_DEFAULT_VERSION).map_err(
        |err| {
          err.map_err_context(|| {
            format!("Unable to connect to docker at {path}")
          })
        },
      )?
    }
    docker_host
      if docker_host.starts_with("http://")
        | docker_host.starts_with("https://") =>
    {
      Docker::connect_with_http(docker_host, 120, API_DEFAULT_VERSION).map_err(
        |err| {
          err.map_err_context(|| {
            format!("Unable to connect to docker at {docker_host}")
          })
        },
      )?
    }
    _ => {
      return Err(IoError::invalid_data(
        "Url",
        &format!("{docker_host} have invalid schema"),
      ))
    }
  };
  Ok(docker)
}

/// Hook labels for a container
pub fn hook_labels(
  key: &str,
  namespace: &str,
  labels: &HashMap<String, String>,
) -> HashMap<String, String> {
  let mut hooked_labels = labels.clone();
  hooked_labels.insert("io.nanocl".to_owned(), "enabled".to_owned());
  hooked_labels.insert("io.nanocl.kind".to_owned(), "cargo".to_owned());
  hooked_labels.insert("io.nanocl.c".to_owned(), key.to_owned());
  hooked_labels.insert("io.nanocl.n".to_owned(), namespace.to_owned());
  hooked_labels.insert(
    "com.docker.compose.project".to_owned(),
    format!("nanocl_{namespace}"),
  );
  hooked_labels
}

/// Hook cargoes binds to replace relative path with absolute path
pub fn hook_binds(cargo: &CargoSpecPartial) -> IoResult<CargoSpecPartial> {
  let new_cargo = match &cargo.container.host_config {
    None => cargo.clone(),
    Some(host_config) => match &host_config.binds {
      None => cargo.clone(),
      Some(binds) => {
        let mut new_binds = Vec::new();
        for bind in binds {
          let bind_split = bind.split(':').collect::<Vec<&str>>();
          let new_bind = if bind_split.len() == 2 {
            let host_path = bind_split[0];
            if host_path.starts_with('.') {
              let curr_path = std::env::current_dir()?;
              let path = std::path::Path::new(&curr_path)
                .join(std::path::PathBuf::from(host_path));
              let path = path.display().to_string();
              format!("{}:{}", path, bind_split[1])
            } else {
              bind.clone()
            }
          } else {
            bind.clone()
          };
          new_binds.push(new_bind);
        }
        CargoSpecPartial {
          container: Config {
            host_config: Some(HostConfig {
              binds: Some(new_binds),
              ..host_config.clone()
            }),
            ..cargo.container.clone()
          },
          ..cargo.clone()
        }
      }
    },
  };
  Ok(new_cargo)
}

/// Create a container from a cargo config
pub async fn create_cargo_container(
  cargo: &CargoSpecPartial,
  namespace: &str,
  docker: &Docker,
) -> IoResult<ContainerCreateResponse> {
  let hooked_cargo = hook_binds(cargo)?;
  let name = &hooked_cargo.name;
  let config = &hooked_cargo.container;
  let key = format!("{name}.{namespace}");
  let host_config = config.host_config.clone().unwrap_or_default();
  let hooked_container = Config {
    labels: Some(hook_labels(
      &key,
      namespace,
      &config.labels.clone().unwrap_or_default(),
    )),
    host_config: Some(HostConfig {
      network_mode: Some(
        host_config
          .network_mode
          .clone()
          .unwrap_or(namespace.to_owned()),
      ),
      restart_policy: Some(RestartPolicy {
        name: Some(RestartPolicyNameEnum::ALWAYS),
        ..Default::default()
      }),
      ..host_config
    }),
    ..config.clone()
  };
  println!("creating : {hooked_container:#?}");
  let container = docker
    .create_container(
      Some(CreateContainerOptions {
        name: format!("{key}.c"),
        ..Default::default()
      }),
      hooked_container,
    )
    .await
    .map_err(|err| err.map_err_context(|| format!("Cargo {name}")))?;
  Ok(container)
}

/// Detect docker host from docker config
pub fn detect_docker_host() -> IoResult<(String, bool)> {
  let home = std::env::var("HOME").map_err(|_| {
    std::io::Error::new(std::io::ErrorKind::Other, "Could not get $HOME")
  })?;
  let path = format!("{home}/.docker/config.json");
  let Ok(str) = std::fs::read_to_string(path) else {
    return Ok(("unix:///var/run/docker.sock".into(), false));
  };
  let config =
    serde_json::from_str::<serde_json::Value>(&str).map_err(|err| {
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("Could not parse docker config: {err}"),
      )
    })?;
  let context = config["currentContext"].as_str().unwrap_or("default");
  if context == "default" {
    return Ok(("unix:///var/run/docker.sock".into(), false));
  }
  let hash = hash::calculate_SHA256(context);
  let path = format!("{home}/.docker/contexts/meta/{hash}/meta.json",);
  let str = std::fs::read_to_string(path)?;
  let config =
    serde_json::from_str::<DockerContextMeta>(&str).map_err(|err| {
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("Could not parse docker config: {err}"),
      )
    })?;
  let Some(endpoint) = config.endpoints.get("docker") else {
    return Ok(("unix:///var/run/docker.sock".into(), false));
  };
  if !endpoint.host.starts_with("unix://") {
    return Err(
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("No unix docker endpoint unsupported yet: {}", endpoint.host),
      )
      .into(),
    );
  }
  if context == "desktop-linux" {
    return Ok((endpoint.host.to_owned(), true));
  }
  Ok((endpoint.host.to_owned(), false))
}
