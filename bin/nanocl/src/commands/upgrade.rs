use std::collections::HashMap;

use futures::StreamExt;
use indicatif::{ProgressBar, MultiProgress};

use nanocl_utils::io_error::{IoError, FromIo, IoResult};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::cargo_config::CargoConfigPartial;

use crate::utils;
use crate::models::UpgradeOpts;
use super::cargo_image::exec_cargo_image_pull;

pub async fn exec_upgrade(
  client: &NanocldClient,
  opts: &UpgradeOpts,
) -> IoResult<()> {
  let config = client.info().await?.config;

  let data = liquid::object!({
    "advertise_addr": config.advertise_addr,
    "state_dir": config.state_dir,
    "docker_host": config.docker_host,
    "gateway": config.gateway,
    "conf_dir": config.conf_dir,
    "hostname": config.hostname,
    "gid": config.gid,
  });

  let installer = utils::installer::get_template(opts.template.clone()).await?;

  let installer = utils::state::compile(&installer, &data)?;

  let data =
    serde_yaml::from_str::<serde_json::Value>(&installer).map_err(|err| {
      err.map_err_context(|| "Unable to convert upgrade to yaml")
    })?;

  let cargoes = serde_json::from_value::<Vec<CargoConfigPartial>>(
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
  }

  let data =
    serde_json::from_value::<serde_json::Value>(data).map_err(|err| {
      err.map_err_context(|| "Unable to convert upgrade to json")
    })?;
  let mut stream = client.apply_state(&data).await?;

  let multiprogress = MultiProgress::new();
  multiprogress.set_move_cursor(false);
  let mut layers: HashMap<String, ProgressBar> = HashMap::new();
  while let Some(res) = stream.next().await {
    let res = res?;
    utils::state::update_progress(&multiprogress, &mut layers, &res.key, &res);
  }
  Ok(())
}
