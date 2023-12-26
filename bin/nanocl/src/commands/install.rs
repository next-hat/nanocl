use std::time::Duration;
use std::collections::HashMap;

use nix::unistd::Group;
use bollard_next::{
  network::{CreateNetworkOptions, InspectNetworkOptions},
  container::StartContainerOptions,
};

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_utils::unix;
use nanocld_client::stubs::statefile::Statefile;

use crate::{
  utils,
  models::{
    InstallOpts, NanocldArg, Context, ContextMetaData, ContextEndpoint,
  },
};

/// This function is called when running `nanocl install`
/// It will install nanocl system containers
pub async fn exec_install(args: &InstallOpts) -> IoResult<()> {
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
        "/var/lib/nanocl".to_owned()
      }
    }
  };
  let conf_dir = args.conf_dir.as_deref().unwrap_or("/etc/nanocl").to_owned();
  let gateway = match &args.gateway {
    None => {
      if is_docker_desktop {
        "127.0.0.1".to_owned()
      } else {
        unix::network::get_default_ip()?.to_string()
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
  Those command can help:\n\
  sudo groupadd {group}\n\
  sudo usermod -aG {group} $USER\n\
  newgrp {group}\n"
      ),
    ))?;
  let hostname = match &args.hostname {
    Some(hostname) => hostname.to_owned(),
    None => unix::network::get_hostname()?,
  };
  let docker_uds_path = if docker_host.starts_with("unix://") {
    Some(docker_host.replace("unix://", ""))
  } else {
    None
  };
  let nanocld_args = NanocldArg {
    docker_host,
    docker_uds_path,
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
  let deployment =
    serde_yaml::from_str::<Statefile>(&installer).map_err(|err| {
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
  let pg_style = utils::progress::create_spinner_style("green");
  for cargo in cargoes {
    let image = cargo.container.image.clone().ok_or(IoError::invalid_data(
      format!("Cargo {} image", cargo.name),
      "is not specified".into(),
    ))?;
    let image_details = image.split(':').collect::<Vec<_>>();
    let [image_name, image_tag] = image_details[..] else {
      return Err(IoError::invalid_data(
        format!("Cargo {} image", cargo.name),
        "invalid format expect image:tag".into(),
      ));
    };
    if docker.inspect_image(&image).await.is_err() || args.force_pull {
      utils::docker::install_image(image_name, image_tag, &docker).await?;
    }
    let pg = utils::progress::create_progress(
      &format!("cargo/{}", &cargo.name),
      &pg_style,
    );
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
    pg.finish();
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
  Ok(())
}
