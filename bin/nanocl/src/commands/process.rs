use nanocl_error::io::IoResult;
use nanocld_client::stubs::process::Process;

use crate::{
  config::CliConfig,
  models::{GenericListOpts, ProcessArg, ProcessFilter, ProcessRow},
};

use super::{GenericCommand, GenericCommandLs};

impl GenericCommand for ProcessArg {
  fn object_name() -> &'static str {
    "processes"
  }
}

impl GenericCommandLs for ProcessArg {
  type Item = ProcessRow;
  type Args = ProcessArg;
  type ApiItem = Process;

  fn get_key(item: &Self::Item) -> String {
    item.key.clone()
  }
}

pub async fn exec_process(
  cli_conf: &CliConfig,
  opts: &GenericListOpts<ProcessFilter>,
) -> IoResult<()> {
  let args = &ProcessArg;
  ProcessArg::exec_ls(&cli_conf.client, args, opts).await
}
