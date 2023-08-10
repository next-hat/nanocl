use nanocl_utils::io_error::{FromIo, IoResult};
use nanocld_client::stubs::state::StateDeployment;

use bollard_next::container::{InspectContainerOptions, RemoveContainerOptions};

use crate::utils;
use crate::config::CommandConfig;
use crate::models::UninstallOpts;

pub async fn exec_uninstall(
  cmd_conf: &CommandConfig<&UninstallOpts>,
) -> IoResult<()> {
  let args = cmd_conf.args;
  let detected_host = utils::docker::detect_docker_host()?;
  let (docker_host, _) = match &args.docker_host {
    Some(docker_host) => (docker_host.to_owned(), args.is_docker_desktop),
    None => detected_host,
  };
  let docker = utils::docker::connect(&docker_host)?;
  let installer = utils::installer::get_template(args.template.clone()).await?;
  let installer = serde_yaml::from_str::<StateDeployment>(&installer)
    .map_err(|err| err.map_err_context(|| "Unable to parse installer"))?;
  let cargoes = installer.cargoes.unwrap_or_default();
  for cargo in cargoes {
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
  }
  println!("Nanocl has been uninstalled successfully!");
  Ok(())
}
