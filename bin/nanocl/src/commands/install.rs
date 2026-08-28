use std::collections::HashMap;
use std::time::Duration;

use bollard_next::{
  container::{LogOutput, LogsOptions, StartContainerOptions},
  network::{CreateNetworkOptions, InspectNetworkOptions},
};
use futures::{StreamExt, stream::FuturesUnordered};
use nix::unistd::Group;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_utils::unix;
use nanocld_client::stubs::statefile::Statefile;

use crate::{
  models::{
    Context, ContextEndpoint, ContextMetaData, InstallOpts, NanocldArg,
    StateRoot,
  },
  utils,
};

#[cfg(target_arch = "x86_64")]
pub(super) fn supports_modern_cockroachdb_image() -> bool {
  const EXTENDED_FEATURES_LEAF: u32 = 0x8000_0001;
  const LAHF_SAHF_BIT: u32 = 1 << 0;

  // Rust does not expose LAHF/SAHF through is_x86_feature_detected!.
  let maximum_extended_leaf = std::arch::x86_64::__cpuid(0x8000_0000).eax;
  let has_lahf_sahf = maximum_extended_leaf >= EXTENDED_FEATURES_LEAF
    && (std::arch::x86_64::__cpuid(EXTENDED_FEATURES_LEAF).ecx & LAHF_SAHF_BIT
      != 0);

  has_lahf_sahf
    && std::is_x86_feature_detected!("cmpxchg16b")
    && std::is_x86_feature_detected!("popcnt")
    && std::is_x86_feature_detected!("sse3")
    && std::is_x86_feature_detected!("ssse3")
    && std::is_x86_feature_detected!("sse4.1")
    && std::is_x86_feature_detected!("sse4.2")
    // x86-64-v3 is cumulative. Rust's AVX detection also verifies that the
    // operating system has enabled XSAVE state for XMM and YMM registers.
    && std::is_x86_feature_detected!("avx")
    && std::is_x86_feature_detected!("avx2")
    && std::is_x86_feature_detected!("bmi1")
    && std::is_x86_feature_detected!("bmi2")
    && std::is_x86_feature_detected!("f16c")
    && std::is_x86_feature_detected!("fma")
    && std::is_x86_feature_detected!("lzcnt")
    && std::is_x86_feature_detected!("movbe")
}

#[cfg(not(target_arch = "x86_64"))]
pub(super) fn supports_modern_cockroachdb_image() -> bool {
  true
}

/// This function is called when running `nanocl install`
/// It will install nanocl system containers
///
pub async fn exec_install(args: &InstallOpts) -> IoResult<()> {
  let home_dir = std::env::var("HOME").map_err(|err| {
    IoError::interrupted("Unable to get $HOME env variable", &err.to_string())
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
    .daemon_hosts
    .clone()
    .unwrap_or(vec!["unix:///run/nanocl/nanocl.sock".into()]);
  let group = Group::from_name(group)
    .map_err(|err| IoError::with_context("Group", err.into()))?
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
  let docker_uds_host_path =
    if is_docker_desktop && docker_host.starts_with("unix://") {
      Some("/var/run/docker.sock.raw".to_owned())
    } else {
      docker_uds_path.clone()
    };
  let nanocld_args = NanocldArg {
    docker_host,
    docker_uds_path,
    docker_uds_host_path,
    state_dir,
    conf_dir,
    gateway,
    hosts,
    hostname,
    advertise_addr,
    is_docker_desktop,
    supports_modern_cockroachdb_image: supports_modern_cockroachdb_image(),
    gid: group.gid.into(),
    home_dir: home_dir.clone(),
    channel: crate::version::CHANNEL.to_owned(),
  };
  let installer = utils::installer::get_template(args.template.clone()).await?;
  let data: liquid::Object = nanocld_args.clone().into();
  let installer = utils::state::compile(&installer, &data, StateRoot::None)?;
  let deployment =
    serde_yaml::from_str::<Statefile>(&installer).map_err(|err| {
      err.map_err_context(|| "Unable to extract deployment from installer")
    })?;
  let cargoes = deployment
    .cargoes
    .ok_or(IoError::invalid_data("Cargoes", "Not founds"))?;
  let docker = utils::docker::connect(&nanocld_args.docker_host)?;
  if docker
    .inspect_network("nanoclbr0", None::<InspectNetworkOptions<String>>)
    .await
    .is_err()
  {
    docker
      .create_network(CreateNetworkOptions {
        name: "nanoclbr0",
        check_duplicate: true,
        driver: "bridge",
        internal: false,
        attachable: true,
        ingress: false,
        enable_ipv6: false,
        ..Default::default()
      })
      .await
      .map_err(|err| {
        err.map_err_context(|| "Unable to create nanoclbr0 network")
      })?;
  }
  for cargo in &cargoes {
    let token = format!("cargo/{}", cargo.name);
    let pg_style = utils::progress::create_spinner_style(&token, "green");
    let pg = utils::progress::create_progress("(submitting)", &pg_style);
    let container = utils::docker::single_application_container(cargo)?;
    let image =
      container
        .container_config
        .image
        .clone()
        .ok_or(IoError::invalid_data(
          format!("Cargo {} image", cargo.name),
          "is not specified".to_owned(),
        ))?;
    let image_details = image.split(':').collect::<Vec<_>>();
    let [image_name, image_tag] = image_details[..] else {
      return Err(IoError::invalid_data(
        format!("Cargo {} image", cargo.name),
        "invalid format expect image:tag".into(),
      ));
    };
    if docker.inspect_image(&image).await.is_err() || args.force_pull {
      pg.set_message("(pulling)");
      utils::docker::install_image(image_name, image_tag, &docker, true)
        .await?;
    }
    pg.set_message("(creating)");
    let container = utils::docker::create_cargo_container(
      cargo,
      &deployment.namespace.clone().unwrap_or("system".into()),
      &docker,
    )
    .await?;
    pg.set_message("(starting)");
    docker
      .start_container(&container.id, None::<StartContainerOptions<String>>)
      .await
      .map_err(|err| {
        err.map_err_context(|| format!("Unable to start cargo {}", cargo.name))
      })?;
    ntex::time::sleep(Duration::from_secs(2)).await;
    pg.finish_with_message("(running)");
  }
  if is_docker_desktop {
    println!("Docker desktop detected");
    println!("Setting up context for docker desktop");
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
            ssl: None,
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
  println!("Nanocl system installed");
  if args.follow {
    cargoes
      .into_iter()
      .map(move |cargo| {
        let docker_ptr = docker.clone();
        async move {
          let opts = LogsOptions::<String> {
            follow: true,
            stdout: true,
            stderr: true,
            ..Default::default()
          };
          let mut stream =
            docker_ptr.logs(&format!("system.{}.c", cargo.name), Some(opts));
          while let Some(log) = stream.next().await {
            match log {
              Ok(log) => {
                match log {
                  LogOutput::StdOut { message } => {
                    let message = String::from_utf8_lossy(&message);
                    print!("{}: {}", cargo.name, message);
                  }
                  LogOutput::StdErr { message } => {
                    let message = String::from_utf8_lossy(&message);
                    eprint!("{}: {}", cargo.name, message);
                  }
                  LogOutput::Console { message } => {
                    let message = String::from_utf8_lossy(&message);
                    print!("{}: {}", cargo.name, message);
                  }
                  _ => {}
                };
              }
              Err(err) => {
                eprintln!("WARN: Unable to get logs: {err}");
              }
            }
          }
        }
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<_>>()
      .await;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn nstore_image(supports_modern_cockroachdb_image: bool) -> String {
    let data = liquid::object!({
      "docker_host": "unix:///var/run/docker.sock",
      "state_dir": "/tmp/state",
      "conf_dir": "/tmp/conf",
      "gateway": "127.0.0.1",
      "hostname": "localhost",
      "advertise_addr": "127.0.0.1",
      "is_docker_desktop": false,
      "supports_modern_cockroachdb_image": supports_modern_cockroachdb_image,
      "gid": 0,
      "home_dir": "/tmp/home",
      "channel": "nightly",
      "docker_uds_path": "/var/run/docker.sock",
      "docker_uds_host_path": "/var/run/docker.sock",
    });
    let installer = utils::state::compile(
      include_str!("../../../../installer.yml"),
      &data,
      StateRoot::None,
    )
    .unwrap();
    let deployment = serde_yaml::from_str::<Statefile>(&installer).unwrap();
    let nstore = deployment
      .cargoes
      .unwrap()
      .into_iter()
      .find(|cargo| cargo.name == "nstore")
      .unwrap();
    utils::docker::single_application_container(&nstore)
      .unwrap()
      .container_config
      .image
      .clone()
      .unwrap()
  }

  #[test]
  fn installer_selects_cockroachdb_image_for_x86_64_v3_support() {
    assert_eq!(
      nstore_image(true),
      "docker.io/cockroachdb/cockroach:v25.4.13"
    );
    assert_eq!(
      nstore_image(false),
      "docker.io/cockroachdb/cockroach:v23.1.30"
    );
  }
}
