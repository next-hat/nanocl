use futures::StreamExt;

use nanocl_utils::io_error::{FromIo, IoResult};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::state::StateStream;

use crate::utils;
use crate::models::DEFAULT_INSTALLER;

pub async fn exec_upgrade(client: &NanocldClient) -> IoResult<()> {
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

  let installer = utils::state::compile(DEFAULT_INSTALLER, &data)?;

  let data = serde_yaml::from_str(&installer).map_err(|err| {
    err.map_err_context(|| "Unable to convert upgrade to yaml")
  })?;

  let data =
    serde_json::from_value::<serde_json::Value>(data).map_err(|err| {
      err.map_err_context(|| "Unable to convert upgrade to json")
    })?;
  let mut stream = client.apply_state(&data).await?;

  while let Some(res) = stream.next().await {
    let res = res?;
    match res {
      StateStream::Error(err) => eprintln!("{err}"),
      StateStream::Msg(msg) => println!("{msg}"),
    }
  }
  Ok(())
}
