use nanocl_error::io::IoResult;
use nanocld_client::stubs::process::{Process, ProcessLogQuery};

use crate::{
  config::CliConfig,
  models::{
    GenericInspectOpts, GenericListOpts, LogsOpts, ProcessArg, ProcessFilter,
    ProcessRow,
  },
  utils,
};

use super::{GenericCommand, GenericCommandInspect, GenericCommandLs};

impl GenericCommandInspect for ProcessArg {
  type ApiItem = Process;
}

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

/// Get logs for a process by name or id
pub async fn logs_process(
  cli_conf: &CliConfig,
  opts: &LogsOpts,
) -> IoResult<()> {
  let query: ProcessLogQuery = opts.clone().into();
  let mut streams = Vec::with_capacity(opts.names.len());
  for name in &opts.names {
    match cli_conf.client.logs_process(name, Some(&query)).await {
      Ok(stream) => streams.push(stream),
      Err(err) => eprintln!("WARN: cannot stream logs for {name}: {err}"),
    }
  }
  utils::print::logs_process_streams(streams).await?;
  Ok(())
}

/// Inspect a process by it's name
pub async fn inspect_process(
  cli_conf: &CliConfig,
  opts: &GenericInspectOpts,
) -> IoResult<()> {
  ProcessArg::exec_inspect(cli_conf, opts).await?;
  Ok(())
}

pub async fn exec_process(
  cli_conf: &CliConfig,
  opts: &GenericListOpts<ProcessFilter>,
) -> IoResult<()> {
  let args = &ProcessArg;
  ProcessArg::exec_ls(&cli_conf.client, args, opts, None).await
}
