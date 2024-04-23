use futures::StreamExt;
use nanocl_error::io::{IoResult, FromIo, IoError};

use nanocld_client::stubs::process::{ProcessLogQuery, ProcessWaitQuery};

use crate::{
  utils,
  config::CliConfig,
  models::{
    JobArg, JobCommand, JobRow, JobRemoveOpts, JobInspectOpts, JobLogsOpts,
    JobWaitOpts, JobStartOpts,
  },
};

use super::GenericList;

impl GenericList for JobArg {
  type Item = JobRow;
  type Args = JobArg;
  type ApiItem = nanocld_client::stubs::job::JobSummary;

  fn object_name() -> &'static str {
    "jobs"
  }

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

/// Execute the `nanocl job rm` command to remove a job
async fn exec_job_rm(
  cli_conf: &CliConfig,
  opts: &JobRemoveOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  if !opts.skip_confirm {
    utils::dialog::confirm(&format!("Delete job  {}?", opts.names.join(",")))
      .map_err(|err| err.map_err_context(|| "Delete job"))?;
  }
  for name in &opts.names {
    client.delete_job(name).await?;
  }
  Ok(())
}

/// Execute the `nanocl job inspect` command to inspect a job
async fn exec_job_inspect(
  cli_conf: &CliConfig,
  opts: &JobInspectOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let job = client.inspect_job(&opts.name).await?;
  let display = opts
    .display
    .clone()
    .unwrap_or(cli_conf.user_config.display_format.clone());
  utils::print::display_format(&display, job)?;
  Ok(())
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

/// Execute the `nanocl job start` command to start a job
async fn exec_job_start(
  cli_conf: &CliConfig,
  opts: &JobStartOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  client.start_process("job", &opts.name, None).await?;
  Ok(())
}

/// Function that execute when running `nanocl job`
pub async fn exec_job(cli_conf: &CliConfig, args: &JobArg) -> IoResult<()> {
  match &args.command {
    JobCommand::List(opts) => {
      JobArg::exec_ls(&cli_conf.client, args, opts).await
    }
    JobCommand::Remove(opts) => exec_job_rm(cli_conf, opts).await,
    JobCommand::Inspect(opts) => exec_job_inspect(cli_conf, opts).await,
    JobCommand::Logs(opts) => exec_job_logs(cli_conf, opts).await,
    JobCommand::Wait(opts) => exec_job_wait(cli_conf, opts).await,
    JobCommand::Start(opts) => exec_job_start(cli_conf, opts).await,
  }
}
