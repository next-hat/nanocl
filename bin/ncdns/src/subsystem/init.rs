use std::sync::Arc;

use nanocl_error::io::IoResult;
use nanocld_client::NanocldClient;

use crate::{
  cli::Cli,
  models::{SystemStateRef, Dnsmasq, SystemState},
};

pub async fn init(cli: &Cli) -> IoResult<SystemStateRef> {
  let conf_dir = cli.conf_dir.to_owned().unwrap_or("/etc".into());
  let dnsmasq = Dnsmasq::new(&conf_dir).with_dns(cli.dns.clone()).ensure()?;
  #[allow(unused)]
  let mut client = NanocldClient::connect_with_unix_default();
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    client = NanocldClient::connect_to("http://nanocl.internal:8585", None);
  }
  super::event::spawn(&client);
  Ok(Arc::new(SystemState { client, dnsmasq }))
}
