use nanocl_utils::io_error::{FromIo, IoResult};
use nanocld_client::stubs::state::StateDeployment;

use bollard_next::container::{InspectContainerOptions, RemoveContainerOptions};

use crate::utils;
use crate::models::UninstallOpts;

pub async fn exec_uninstall(opts: &UninstallOpts) -> IoResult<()> {
  let docker_host = opts
    .docker_host
    .clone()
    .unwrap_or("unix:///run/docker.sock".into());

  let docker = utils::docker::connect(&docker_host)?;

  let installer = utils::installer::get_template(opts.template.clone()).await?;

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
