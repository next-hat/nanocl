use nanocl_error::io::{IoError, FromIo, IoResult};
use nanocld_client::stubs::cargo_spec::CargoSpecPartial;

use crate::{utils, version};
use crate::config::CliConfig;
use crate::models::UpgradeOpts;
use super::cargo_image::exec_cargo_image_pull;

/// ## Exec upgrade
///
/// Function that execute when running `nanocl upgrade`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [args](UpgradeOpts) The upgrade options
///
pub async fn exec_upgrade(
  cli_conf: &CliConfig,
  args: &UpgradeOpts,
) -> IoResult<()> {
  let detected_host = utils::docker::detect_docker_host()?;
  let (docker_host, is_docker_desktop) = match &args.docker_host {
    Some(docker_host) => (docker_host.to_owned(), args.is_docker_desktop),
    None => detected_host,
  };
  let home_dir = std::env::var("HOME").map_err(|err| {
    IoError::interupted("Unable to get $HOME env variable", &err.to_string())
  })?;
  let client = &cli_conf.client;
  let config = client.info().await?.config;
  let data = liquid::object!({
    "advertise_addr": config.advertise_addr,
    "state_dir": config.state_dir,
    "docker_host": docker_host,
    "is_docker_desktop": is_docker_desktop,
    "gateway": config.gateway,
    "conf_dir": config.conf_dir,
    "hostname": config.hostname,
    "hosts": config.hosts.join(" "),
    "gid": config.gid,
    "home_dir": home_dir,
    "channel": version::CHANNEL,
  });
  let installer = utils::installer::get_template(args.template.clone()).await?;
  let installer = utils::state::compile(&installer, &data)?;
  let data =
    serde_yaml::from_str::<serde_json::Value>(&installer).map_err(|err| {
      err.map_err_context(|| "Unable to convert upgrade to yaml")
    })?;
  let cargoes = serde_json::from_value::<Vec<CargoSpecPartial>>(
    data
      .get("Cargoes")
      .cloned()
      .ok_or(IoError::invalid_data("Cargoes", "arent specified"))?,
  )
  .map_err(|err| err.map_err_context(|| "Unable to convert upgrade to json"))?;
  for cargo in cargoes {
    let image = cargo.container.image.clone().ok_or(IoError::invalid_data(
      format!("Cargo {} image", cargo.name),
      "is not specified".into(),
    ))?;
    exec_cargo_image_pull(client, &image).await?;
    print!("Upgrading {}", cargo.name);
    let _ = client
      .put_cargo(&cargo.name.clone(), &cargo, Some("system"))
      .await;
    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
    println!(" {} has been upgraded successfully!", cargo.name);
  }
  Ok(())
}
