use nanocl_error::io::IoResult;
use nanocld_client::NanocldClient;

use crate::cli::Cli;
use crate::nginx::Nginx;

use super::event;
use super::network_log;

pub async fn init(cli: &Cli) -> IoResult<(Nginx, NanocldClient)> {
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
  }
  let nginx = Nginx::new(&cli.conf_dir.clone().unwrap_or("/etc/nginx".into()));
  nginx.ensure().await?;
  event::spawn(&nginx, &client);
  network_log::spawn();
  Ok((nginx, client))
}
