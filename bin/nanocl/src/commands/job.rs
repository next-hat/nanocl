use futures::StreamExt;
use nanocl_error::io::{IoResult, FromIo, IoError};

use nanocld_client::stubs::{
  job::JobInspect,
  process::{ProcessLogQuery, ProcessWaitQuery},
};

use crate::{
  utils,
  config::CliConfig,
  models::{
    GenericDefaultOpts, JobArg, JobCommand, JobLogsOpts, JobRow, JobWaitOpts,
  },
};

use super::{
  GenericCommand, GenericCommandInspect, GenericCommandLs, GenericCommandRm,
  GenericCommandStart,
};

impl GenericCommand for JobArg {
  fn object_name() -> &'static str {
    "jobs"
  }
}

impl GenericCommandLs for JobArg {
  type Item = JobRow;
  type Args = JobArg;
  type ApiItem = nanocld_client::stubs::job::JobSummary;

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

impl GenericCommandRm<GenericDefaultOpts, String> for JobArg {}

impl GenericCommandStart for JobArg {}

impl GenericCommandInspect for JobArg {
  type ApiItem = JobInspect;
}

/// Execute the `nanocl job logs` command to list the logs of a job
async fn exec_job_logs(
  cli_conf: &CliConfig,
  opts: &JobLogsOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let query = ProcessLogQuery {
    namespace: None,
    tail: opts.tail.clone(),
    since: opts.since,
    until: opts.until,
    follow: Some(opts.follow),
    timestamps: Some(opts.timestamps),
    stderr: None,
    stdout: None,
  };
  let stream = client
    .logs_processes("job", &opts.name, Some(&query))
    .await?;
  utils::print::logs_process_stream(stream).await?;
  Ok(())
}

/// Execute the `nanocl job wait` command to wait for a job to finish
async fn exec_job_wait(
  cli_conf: &CliConfig,
  opts: &JobWaitOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut stream = client
    .wait_process(
      "job",
      &opts.name,
      Some(&ProcessWaitQuery {
        condition: opts.condition.clone(),
        namespace: None,
      }),
    )
    .await?;
  let mut has_error = false;
  while let Some(chunk) = stream.next().await {
    let resp = match chunk {
      Ok(ref chunk) => chunk,
      Err(e) => return Err(e.map_err_context(|| "Stream logs").into()),
    };
    if resp.status_code != 0 {
      eprintln!(
        "Job container {}-{} ended with error code {}",
        opts.name, resp.process_name, resp.status_code,
      );
      has_error = true;
    }
  }
  if has_error {
    return Err(IoError::other("Job wait", "task ended with error"));
  }
  Ok(())
}

/// Function that execute when running `nanocl job`
pub async fn exec_job(cli_conf: &CliConfig, args: &JobArg) -> IoResult<()> {
  match &args.command {
    JobCommand::List(opts) => {
      JobArg::exec_ls(&cli_conf.client, args, opts).await
    }
    JobCommand::Remove(opts) => {
      JobArg::exec_rm(&cli_conf.client, opts, None).await
    }
    JobCommand::Inspect(opts) => {
      JobArg::exec_inspect(cli_conf, opts, None).await
    }
    JobCommand::Logs(opts) => exec_job_logs(cli_conf, opts).await,
    JobCommand::Wait(opts) => exec_job_wait(cli_conf, opts).await,
    JobCommand::Start(opts) => {
      JobArg::exec_start(&cli_conf.client, opts, None).await
    }
  }
}
