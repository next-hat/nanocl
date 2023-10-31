use futures::StreamExt;
use nanocld_client::bollard_next;
use nanocld_client::NanocldClient;
use nanocl_utils::io_error::{IoResult, FromIo, IoError};

use nanocld_client::stubs::cargo::{CargoInspect, CreateExecOptions};
use nanocld_client::stubs::proxy::ProxySsl;
use nanocld_client::stubs::proxy::ProxySslConfig;
use nanocld_client::stubs::resource::{ResourceQuery, ResourcePartial};
use nanocld_client::stubs::proxy::{
  ProxyRule, StreamTarget, ProxyStreamProtocol, ProxyRuleHttp, UpstreamTarget,
  ProxyHttpLocation, ProxyRuleStream, LocationTarget, ResourceProxyRule,
};
use nanocld_client::stubs::vm::VmInspect;

#[cfg(test)]
pub(crate) mod tests {
  use std::process::Output;
  use ntex::web::error::BlockingError;

  use nanocl_utils::logger;

  // Before a test
  pub fn before() {
    // Build a test env logger
    std::env::set_var("TEST", "true");
    logger::enable_logger("ncproxy");
  }

  pub async fn exec_nanocl(arg: &str) -> std::io::Result<Output> {
    let arg = arg.to_owned();
    ntex::web::block(move || {
      let mut cmd = std::process::Command::new("cargo");
      let mut args = vec!["make", "run-cli"];
      args.extend(arg.split(' ').collect::<Vec<&str>>());
      cmd.args(&args);
      let output = cmd.output()?;
      Ok::<_, std::io::Error>(output)
    })
    .await
    .map_err(|err| match err {
      BlockingError::Error(err) => err,
      BlockingError::Canceled => {
        std::io::Error::new(std::io::ErrorKind::Other, "Canceled")
      }
    })
  }
}
