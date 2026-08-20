use std::collections::HashMap;

use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use bollard_next::container::CreateContainerOptions;
use bollard_next::image::CreateImageOptions;
use bollard_next::models::EmptyObject;
use bollard_next::service::{
  ContainerCreateResponse, HostConfig, ProgressDetail, RestartPolicy,
  RestartPolicyNameEnum,
};
use bollard_next::{API_DEFAULT_VERSION, Docker};

use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::stubs::cargo_spec::{
  CARGO_CONTAINER_POSITION_LABEL, CargoSpec, Config, ContainerSpec,
};

use crate::models::DockerContextMeta;
use crate::utils::hash;
use crate::utils::math::calculate_percentage;

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
  disable_progress: bool,
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
    if disable_progress {
      continue;
    }
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
        if !layers.contains_key(&id) {
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
      #[cfg(not(target_os = "windows"))]
      {
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
      #[cfg(target_os = "windows")]
      {
        return Err(IoError::invalid_data(
          "Url",
          "Unix socket file is not supported on windows",
        ));
      }
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
      ));
    }
  };
  Ok(docker)
}

/// Hook labels for a container
pub fn hook_labels(
  key: &str,
  namespace: &str,
  replica_id: &str,
  container: &ContainerSpec,
  labels: &HashMap<String, String>,
) -> HashMap<String, String> {
  let mut hooked_labels = labels.clone();
  hooked_labels.insert("io.nanocl".to_owned(), "enabled".to_owned());
  hooked_labels.insert("io.nanocl.kind".to_owned(), "cargo".to_owned());
  hooked_labels.insert("io.nanocl.c".to_owned(), key.to_owned());
  hooked_labels.insert("io.nanocl.n".to_owned(), namespace.to_owned());
  hooked_labels.insert("io.nanocl.not-init-c".to_owned(), "true".to_owned());
  hooked_labels
    .insert("io.nanocl.cargo.replica".to_owned(), replica_id.to_owned());
  hooked_labels
    .insert("io.nanocl.cargo.replica-ordinal".to_owned(), "0".to_owned());
  hooked_labels.insert(
    "io.nanocl.cargo.container".to_owned(),
    container.name.clone(),
  );
  hooked_labels.insert("io.nanocl.cargo.role".to_owned(), "app".to_owned());
  hooked_labels.insert(
    "io.nanocl.cargo.essential".to_owned(),
    container.essential.to_string(),
  );
  hooked_labels
    .insert(CARGO_CONTAINER_POSITION_LABEL.to_owned(), "0".to_owned());
  hooked_labels.insert(
    "com.docker.compose.project".to_owned(),
    format!("nanocl_{namespace}"),
  );
  hooked_labels
}

fn hook_config_binds(config: &Config) -> IoResult<Config> {
  let Some(host_config) = &config.host_config else {
    return Ok(config.clone());
  };
  let Some(binds) = &host_config.binds else {
    return Ok(config.clone());
  };
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
  Ok(Config {
    host_config: Some(HostConfig {
      binds: Some(new_binds),
      ..host_config.clone()
    }),
    ..config.clone()
  })
}

fn hook_container_binds(container: &ContainerSpec) -> IoResult<ContainerSpec> {
  Ok(ContainerSpec {
    container_config: hook_config_binds(&container.container_config)?,
    ..container.clone()
  })
}

/// Hook Cargo binds to replace relative paths with absolute paths.
pub fn hook_binds(cargo: &CargoSpec) -> IoResult<CargoSpec> {
  Ok(CargoSpec {
    init_containers: cargo
      .init_containers
      .iter()
      .map(hook_container_binds)
      .collect::<IoResult<Vec<_>>>()?,
    containers: cargo
      .containers
      .iter()
      .map(hook_container_binds)
      .collect::<IoResult<Vec<_>>>()?,
    ..cargo.clone()
  })
}

/// Select the only application container supported by the local installer.
pub fn single_application_container(
  cargo: &CargoSpec,
) -> IoResult<&ContainerSpec> {
  match cargo.containers.as_slice() {
    [container] => Ok(container),
    [] => Err(IoError::invalid_data(
      format!("Cargo {} containers", cargo.name),
      "no application container is declared".to_owned(),
    )),
    _ => Err(IoError::invalid_data(
      format!("Cargo {} containers", cargo.name),
      "the local installer supports exactly one application container"
        .to_owned(),
    )),
  }
}

/// Project final Cargo-level networking onto the local installer's one
/// application container.
fn installer_container_config(
  cargo: &CargoSpec,
  container: &ContainerSpec,
) -> Config {
  let mut config = container.container_config.clone();
  let mut host_config = config.host_config.clone().unwrap_or_default();
  if !matches!(host_config.network_mode.as_deref(), Some("host" | "none")) {
    host_config.network_mode = Some(
      cargo
        .network_mode
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "nanoclbr0".to_owned()),
    );
  }
  if let Some(port_bindings) = &cargo.port_bindings {
    host_config.port_bindings = Some(port_bindings.clone());
    let exposed_ports = config.exposed_ports.get_or_insert_default();
    for port in port_bindings.keys() {
      exposed_ports
        .entry(port.clone())
        .or_insert_with(EmptyObject::default);
    }
  }
  config.host_config = Some(host_config);
  config
}

/// Create a container from a cargo config
pub async fn create_cargo_container(
  cargo: &CargoSpec,
  namespace: &str,
  docker: &Docker,
) -> IoResult<ContainerCreateResponse> {
  let hooked_cargo = hook_binds(cargo)?;
  let name = &hooked_cargo.name;
  let declared = single_application_container(&hooked_cargo)?;
  let config = installer_container_config(&hooked_cargo, declared);
  let key =
    nanocld_client::stubs::resource_key::ResourceKey::new(name, namespace)
      .map_err(|err| IoError::invalid_input("Cargo key", &err.to_string()))?
      .to_string();
  let host_config = config.host_config.clone().unwrap_or_default();
  let replica_id = uuid::Uuid::new_v4().to_string();
  let hooked_container = Config {
    labels: Some(hook_labels(
      &key,
      namespace,
      &replica_id,
      declared,
      &config.labels.clone().unwrap_or_default(),
    )),
    host_config: Some(HostConfig {
      network_mode: Some(
        host_config.network_mode.unwrap_or("nanoclbr0".to_owned()),
      ),
      restart_policy: Some(RestartPolicy {
        name: Some(RestartPolicyNameEnum::ALWAYS),
        ..Default::default()
      }),
      ..host_config
    }),
    ..config
  };
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
  let home = std::env::var("HOME")
    .map_err(|_| std::io::Error::other("Could not get $HOME"))?;
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

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::cargo_spec::{CargoNetworkMode, PortBinding};

  use super::*;

  fn container(config: Config) -> ContainerSpec {
    ContainerSpec {
      name: "api".to_owned(),
      essential: true,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: Default::default(),
      container_config: config,
    }
  }

  #[test]
  fn installer_projects_cargo_network_and_ports_without_losing_config() {
    let port_bindings = HashMap::from([(
      "8080/tcp".to_owned(),
      Some(vec![PortBinding {
        host_ip: Some("127.0.0.1".to_owned()),
        host_port: Some("8080".to_owned()),
      }]),
    )]);
    let cargo = CargoSpec {
      name: "api".to_owned(),
      network_mode: Some(CargoNetworkMode::new("private").unwrap()),
      port_bindings: Some(port_bindings.clone()),
      ..Default::default()
    };
    let declared = container(Config {
      env: Some(vec!["KEEP=yes".to_owned()]),
      exposed_ports: Some(HashMap::from([(
        "9000/tcp".to_owned(),
        EmptyObject::default(),
      )])),
      host_config: Some(HostConfig {
        binds: Some(vec!["data:/data".to_owned()]),
        privileged: Some(true),
        ..Default::default()
      }),
      ..Default::default()
    });

    let config = installer_container_config(&cargo, &declared);
    assert_eq!(
      config.env.as_deref(),
      Some(["KEEP=yes".to_owned()].as_slice())
    );
    let host = config.host_config.unwrap();
    assert_eq!(host.network_mode.as_deref(), Some("private"));
    assert_eq!(host.port_bindings, Some(port_bindings));
    assert_eq!(
      host.binds.as_deref(),
      Some(["data:/data".to_owned()].as_slice())
    );
    assert_eq!(host.privileged, Some(true));
    let exposed = config.exposed_ports.unwrap();
    assert!(exposed.contains_key("8080/tcp"));
    assert!(exposed.contains_key("9000/tcp"));
  }

  #[test]
  fn installer_preserves_valid_application_network_escapes() {
    let cargo = CargoSpec {
      name: "api".to_owned(),
      network_mode: Some(CargoNetworkMode::new("private").unwrap()),
      ..Default::default()
    };

    for mode in ["host", "none"] {
      let declared = container(Config {
        host_config: Some(HostConfig {
          network_mode: Some(mode.to_owned()),
          ..Default::default()
        }),
        ..Default::default()
      });
      let config = installer_container_config(&cargo, &declared);
      assert_eq!(
        config
          .host_config
          .and_then(|host_config| host_config.network_mode)
          .as_deref(),
        Some(mode)
      );
    }
  }

  #[test]
  fn installer_labels_describe_one_ordered_application_replica() {
    let declared = container(Config {
      image: Some("alpine:latest".to_owned()),
      ..Default::default()
    });
    let labels = hook_labels(
      "system.api",
      "system",
      "7a9778c5-6e52-49a0-99f8-b63bb15a4337",
      &declared,
      &HashMap::from([("example.owner".to_owned(), "platform".to_owned())]),
    );

    assert_eq!(labels["io.nanocl"], "enabled");
    assert_eq!(labels["io.nanocl.kind"], "cargo");
    assert_eq!(labels["io.nanocl.c"], "system.api");
    assert_eq!(
      labels["io.nanocl.cargo.replica"],
      "7a9778c5-6e52-49a0-99f8-b63bb15a4337"
    );
    assert_eq!(labels["io.nanocl.cargo.replica-ordinal"], "0");
    assert_eq!(labels["io.nanocl.cargo.container"], "api");
    assert_eq!(labels["io.nanocl.cargo.role"], "app");
    assert_eq!(labels["io.nanocl.cargo.essential"], "true");
    assert_eq!(labels[CARGO_CONTAINER_POSITION_LABEL], "0");
    assert_eq!(labels["io.nanocl.not-init-c"], "true");
    assert_eq!(labels["example.owner"], "platform");
  }
}
