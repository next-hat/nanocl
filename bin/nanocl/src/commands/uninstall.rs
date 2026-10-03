use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocld_client::stubs::statefile::Statefile;

use bollard_next::container::{
  InspectContainerOptions, RemoveContainerOptions,
};

use crate::{
  models::{StateOutput, StateRoot, UninstallOpts},
  utils, version,
};

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
    "docker_uds_path": docker_host.replace("unix://", ""),
    "gateway": "127.0.0.1",
    "hosts": "tcp://127.0.0.1:8585",
    "hostname": "localhost",
    "advertise_addr": "127.0.0.1:8585",
    "is_docker_desktop": is_docker_desktop,
    "supports_modern_cockroachdb_image":
      super::install::supports_modern_cockroachdb_image(),
    "gid": "0",
    "home_dir": "/tmp/random",
    "channel": version::CHANNEL.to_owned(),
  });
  let installer = utils::state::compile(&installer, &data, StateRoot::None)?;
  let installer = serde_yaml::from_str::<Statefile>(&installer)
    .map_err(|err| err.map_err_context(|| "Unable to parse installer"))?;
  let cargoes = installer.cargoes.unwrap_or_default();
  let output = args.json.then(|| StateOutput {
    operation: "uninstall",
    statefile: args.template.clone(),
  });
  let (progress, summary) = utils::progress::create_state_progress(
    cargoes.len() as u64,
    "Uninstalling",
    output.as_ref(),
  )?;
  let result = async {
    let progress = &progress;
    let summary = &summary;
    let output = output.as_ref();
    let docker = &docker;
    for cargo in cargoes {
      let token = format!("cargo/system.{}", cargo.name);
      utils::progress::run_state_step(
        progress,
        summary,
        &token,
        output,
        None,
        |pg| async move {
          let key = format!("system.{}.c", &cargo.name);
          if docker
            .inspect_container(&key, None::<InspectContainerOptions>)
            .await
            .is_err()
          {
            return Ok("Unchanged");
          }
          utils::progress::set_state_message(&pg, summary, "Removing", output)?;
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
          Ok("Destroyed")
        },
      )
      .await?;
    }
    Ok::<_, IoError>(())
  }
  .await;
  utils::progress::finish_state_progress(
    &summary,
    if result.is_ok() {
      "Uninstalled"
    } else {
      "Uninstall failed"
    },
    result.is_err(),
    output.as_ref(),
  )?;
  result?;
  Ok(())
}
