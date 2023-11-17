use std::time::Duration;
use std::collections::HashMap;

use nix::unistd::Group;
use bollard_next::container::StartContainerOptions;
use bollard_next::network::{CreateNetworkOptions, InspectNetworkOptions};

use nanocl_utils::unix;
use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocld_client::stubs::state::StateDeployment;

use crate::utils;
use crate::models::{
  InstallOpts, NanocldArg, Context, ContextMetaData, ContextEndpoint,
};

/// ## Exec install
///
/// This function is called when running `nanocl install`
/// It will install nanocl system containers
///
/// ## Arguments
///
/// * [args](InstallOpts) The command arguments
///
pub async fn exec_install(args: &InstallOpts) -> IoResult<()> {
  println!("Installing Nanocl components on your system");
  let home_dir = std::env::var("HOME").map_err(|err| {
    IoError::interupted("Unable to get $HOME env variable", &err.to_string())
  })?;
  let detected_host = utils::docker::detect_docker_host()?;
  let (docker_host, is_docker_desktop) = match &args.docker_host {
    Some(docker_host) => (docker_host.to_owned(), args.is_docker_desktop),
    None => detected_host,
  };
  let state_dir = match &args.state_dir {
    Some(state_dir) => state_dir.to_owned(),
    None => {
      if is_docker_desktop {
        format!("{}/.nanocl/state", home_dir)
      } else {
        "/var/lib/nanocl".into()
      }
    }
  };
  let conf_dir = args.conf_dir.as_deref().unwrap_or("/etc/nanocl").to_owned();
  let gateway = match &args.gateway {
    None => {
      if is_docker_desktop {
        "127.0.0.1".into()
      } else {
        let gateway = unix::network::get_default_ip()?;
        println!("Using default gateway: {}", gateway);
        gateway.to_string()
      }
    }
    Some(gateway) => gateway.clone(),
  };
  let advertise_addr = match &args.advertise_addr {
    None => gateway.clone(),
    Some(advertise_addr) => advertise_addr.clone(),
  };
  let group = args.group.as_deref().unwrap_or("nanocl");
  let hosts = args
    .deamon_hosts
    .clone()
    .unwrap_or(vec!["unix:///run/nanocl/nanocl.sock".into()]);
  let group = Group::from_name(group)
    .map_err(|err| IoError::new("Group", err.into()))?
    .ok_or(IoError::not_found(
      "Group",
      &format!(
        "Error cannot find group: {group}\n\
  You can create it with: sudo groupadd {group}\n\
  And be sure to add yourself to it: sudo usermod -aG {group} $USER\n\
  Then update your current session: newgrp {group}\n\
  And try again"
      ),
    ))?;
  let hostname = if let Some(hostname) = &args.hostname {
    hostname.to_owned()
  } else {
    let hostname = unix::network::get_hostname()?;
    println!("Using default hostname: {hostname}");
    hostname
  };
  let nanocld_args = NanocldArg {
    docker_host,
    state_dir,
    conf_dir,
    gateway,
    hosts,
    hostname,
    advertise_addr,
    is_docker_desktop,
    gid: group.gid.into(),
    home_dir: home_dir.clone(),
    channel: crate::version::CHANNEL.to_owned(),
  };
  let installer = utils::installer::get_template(args.template.clone()).await?;
  let data: liquid::Object = nanocld_args.clone().into();
  let installer = utils::state::compile(&installer, &data)?;
  let deployment = serde_yaml::from_str::<StateDeployment>(&installer)
    .map_err(|err| {
      err.map_err_context(|| "Unable to extract deployment from installer")
    })?;
  let cargoes = deployment
    .cargoes
    .ok_or(IoError::invalid_data("Cargoes", "Not founds"))?;
  let docker = utils::docker::connect(&nanocld_args.docker_host)?;
  if docker
    .inspect_network("system", None::<InspectNetworkOptions<String>>)
    .await
    .is_err()
  {
    println!("Creating system network");
    let mut options = HashMap::new();
    options.insert("com.docker.network.bridge.name", "nanocl.system");
    docker
      .create_network(CreateNetworkOptions {
        name: "system",
        driver: "bridge",
        options,
        ..Default::default()
      })
      .await
      .map_err(|err| err.map_err_context(|| "Nanocl system network"))?;
  }
  for cargo in cargoes {
    let image = cargo.container.image.clone().ok_or(IoError::invalid_data(
      format!("Cargo {} image", cargo.name),
      "is not specified".into(),
    ))?;
    let mut image_detail = image.split(':');
    let from_image = image_detail.next().ok_or(IoError::invalid_data(
      format!("Cargo {} image", cargo.name),
      "invalid format expect image:tag".into(),
    ))?;
    let tag = image_detail.next().ok_or(IoError::invalid_data(
      format!("Cargo {} image", cargo.name),
      "invalid format expect image:tag".into(),
    ))?;
    if docker.inspect_image(&image).await.is_err() {
      utils::docker::install_image(from_image, tag, &docker).await?;
    }
    let container = utils::docker::create_cargo_container(
      &cargo,
      &deployment.namespace.clone().unwrap_or("system".into()),
      &docker,
    )
    .await?;
    docker
      .start_container(&container.id, None::<StartContainerOptions<String>>)
      .await
      .map_err(|err| {
        err.map_err_context(|| format!("Unable to start cargo {}", cargo.name))
      })?;
    ntex::time::sleep(Duration::from_secs(2)).await;
  }
  if is_docker_desktop {
    let context = Context {
      name: "desktop-linux".into(),
      meta_data: ContextMetaData {
        description: "Docker desktop".into(),
      },
      endpoints: {
        let mut map = HashMap::new();
        map.insert(
          "Nanocl".into(),
          ContextEndpoint {
            host: format!("unix://{home_dir}/.nanocl/run/nanocl.sock"),
          },
        );
        map
      },
    };
    if let Err(err) = Context::write(&context) {
      eprintln!("WARN: Unable to create context for docker desktop: {err}");
    }
    if let Err(err) = Context::r#use("desktop-linux") {
      eprintln!("WARN: Unable to use context for docker desktop: {err}");
    }
  }
  println!("Nanocl have been installed successfully!");
  Ok(())
}
