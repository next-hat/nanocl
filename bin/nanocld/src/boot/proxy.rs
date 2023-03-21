use bollard_next::Docker;
use bollard_next::container::{
  CreateContainerOptions, StartContainerOptions, InspectContainerOptions,
};
use nanocl_stubs::state::StateDeployment;

use crate::error::CliError;
use crate::utils;

pub const PROXY_CONF: &str =
  include_str!("../../specs/controllers/dev.proxy.yml");

pub async fn init(docker: &Docker) -> Result<(), CliError> {
  let proxy_conf = serde_yaml::from_str::<StateDeployment>(PROXY_CONF)
    .map_err(|err| CliError {
      msg: format!("Failed to parse proxy config: {}", err),
      code: 4,
    })?;

  let namespace = proxy_conf.namespace.unwrap_or("default".into());
  for cargo in proxy_conf.cargoes.unwrap_or_default() {
    let key = utils::key::gen_key(&namespace, &cargo.name);
    let name = format!("{key}.c");
    let mut cargo = utils::state::hook_cargo_binds(&cargo)?;
    cargo = utils::state::hook_labels(&namespace, &cargo);
    if docker
      .inspect_container(&name, None::<InspectContainerOptions>)
      .await
      .is_ok()
    {
      continue;
    }

    println!("Starting cargo {cargo:#?}");
    let cnt = docker
      .create_container(
        Some(CreateContainerOptions {
          name,
          platform: None,
        }),
        cargo.container,
      )
      .await?;
    docker
      .start_container(&cnt.id, None::<StartContainerOptions<String>>)
      .await?;
  }

  Ok(())
}
