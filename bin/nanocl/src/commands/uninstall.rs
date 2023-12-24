use nanocl_error::io::{FromIo, IoResult};
use nanocld_client::stubs::statefile::Statefile;

use bollard_next::container::{InspectContainerOptions, RemoveContainerOptions};

use crate::{utils, version};
use crate::models::UninstallOpts;

/// This function is called when running `nanocl uninstall`.
/// It will remove nanocl system containers but not the images
/// It will keep existing cargoes, virtual machines and volumes
pub async fn exec_uninstall(args: &UninstallOpts) -> IoResult<()> {
  let detected_host = utils::docker::detect_docker_host()?;
  let (docker_host, is_docker_desktop) = match &args.docker_host {
    Some(docker_host) => (docker_host.to_owned(), args.is_docker_desktop),
    None => detected_host,
  };
  let docker = utils::docker::connect(&docker_host)?;
  let installer = utils::installer::get_template(args.template.clone()).await?;
  let data = liquid::object!({
    "docker_host": docker_host,
    "state_dir": "/tmp/random",
    "conf_dir": "/tmp/random",
    "is_docker_uds": docker_host.starts_with("unix://"),
    "gateway": "127.0.0.1",
    "hosts": "tcp://127.0.0.1:8585",
    "hostname": "localhost",
    "advertise_addr": "127.0.0.1:8585",
    "is_docker_desktop": is_docker_desktop,
    "gid": "0",
    "home_dir": "/tmp/random",
    "channel": version::CHANNEL.to_owned(),
  });
  let installer = utils::state::compile(&installer, &data)?;
  let installer = serde_yaml::from_str::<Statefile>(&installer)
    .map_err(|err| err.map_err_context(|| "Unable to parse installer"))?;
  let cargoes = installer.cargoes.unwrap_or_default();
  let pg_style = utils::progress::create_spinner_style("red");
  for cargo in cargoes {
    let pg = utils::progress::create_progress(
      &format!("cargo/{}", cargo.name),
      &pg_style,
    );
    let key = format!("{}.system.c", &cargo.name);
    if docker
      .inspect_container(&key, None::<InspectContainerOptions>)
      .await
      .is_err()
    {
      continue;
    };
    docker
      .remove_container(
        &key,
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
      )
      .await
      .map_err(|err| {
        err.map_err_context(|| {
          format!("Unable to remove container {}", &cargo.name)
        })
      })?;
    pg.finish();
  }
  Ok(())
}
