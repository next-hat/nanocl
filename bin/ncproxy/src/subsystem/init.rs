use nanocl_error::io::IoResult;

use crate::cli::Cli;
use crate::nginx::Nginx;

use super::event;
use super::network_log;

pub async fn init(cli: &Cli) -> IoResult<Nginx> {
  let nginx = Nginx::new(&cli.conf_dir.clone().unwrap_or("/etc/nginx".into()));
  nginx.ensure().await?;
  event::spawn(&nginx);
  network_log::spawn();
  Ok(nginx)
}
