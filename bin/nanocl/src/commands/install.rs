use std::time::Duration;
use std::collections::HashMap;

use users::get_group_by_name;
use bollard_next::container::StartContainerOptions;
use bollard_next::network::{CreateNetworkOptions, InspectNetworkOptions};

use nanocl_utils::unix;
use nanocl_utils::io_error::{IoError, IoResult, FromIo};
use nanocld_client::stubs::state::StateDeployment;

use crate::utils;
use crate::models::{InstallOpts, NanocldArgs};

/// Execute install command
pub async fn exec_install(opts: &InstallOpts) -> IoResult<()> {
  println!("Installing Nanocl components on your system");

  let docker_host = opts
    .docker_host
    .as_deref()
    .unwrap_or("unix:///var/run/docker.sock")
    .to_owned();

  let state_dir = opts
    .state_dir
    .as_deref()
    .unwrap_or("/var/lib/nanocl")
    .to_owned();

  let conf_dir = opts.conf_dir.as_deref().unwrap_or("/etc/nanocl").to_owned();

  let gateway = match &opts.gateway {
    None => {
      let gateway = unix::network::get_default_ip()?;
      println!("Using default gateway: {}", gateway);
      gateway.to_string()
    }
    Some(gateway) => gateway.clone(),
  };

  let advertise_addr = match &opts.advertise_addr {
    None => gateway.clone(),
    Some(advertise_addr) => advertise_addr.clone(),
  };

  let group = opts.group.as_deref().unwrap_or("nanocl");

  let hosts = opts
    .deamon_hosts
    .clone()
    .unwrap_or(vec!["unix:///run/nanocl/nanocl.sock".into()]);

  let gid = get_group_by_name(group).ok_or(IoError::not_fount(
    "Group",
    &format!(
      "Error cannot find group: {group}\n\
  You can create it with: sudo groupadd {group}\n\
  And be sure to add yourself to it: sudo usermod -aG {group} $USER\n\
  Then update your current session: newgrp {group}\n\
  And try again"
    ),
  ))?;

  let hostname = if let Some(hostname) = &opts.hostname {
    hostname.to_owned()
  } else {
    let hostname = unix::network::get_hostname()?;
    println!("Using default hostname: {hostname}");
    hostname
  };

  let args = NanocldArgs {
    docker_host,
    state_dir,
    conf_dir,
    gateway,
    hosts,
    gid: gid.gid(),
    hostname,
    advertise_addr,
  };

  let installer = utils::installer::get_template(opts.template.clone()).await?;

  let data: liquid::Object = args.clone().into();
  let installer = utils::state::compile(&installer, &data)?;

  let deployment = serde_yaml::from_str::<StateDeployment>(&installer)
    .map_err(|err| {
      err.map_err_context(|| "Unable to extract deployment from installer")
    })?;

  let cargoes = deployment
    .cargoes
    .ok_or(IoError::invalid_data("Cargoes", "Not founds"))?;

  let docker = utils::docker::connect(&args.docker_host)?;

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
    utils::docker::install_image(from_image, tag, &docker).await?;
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

  println!("Nanocl have been installed successfully!");
  Ok(())
}
