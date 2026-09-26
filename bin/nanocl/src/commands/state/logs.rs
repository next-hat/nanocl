use std::time::Duration;

use futures::{StreamExt, join, stream::FuturesUnordered};

use nanocl_error::io::IoResult;
use nanocld_client::{
  NanocldClient,
  stubs::{
    cargo_spec::CargoSpec,
    job::JobPartial,
    process::{Process, ProcessLogQuery},
    statefile::Statefile,
    system::NativeEventAction,
  },
};

use crate::{
  config::CliConfig,
  models::{StateLogsOpts, StateRef},
  utils,
};

use super::{
  ArgParseMode, parse_build_args, parse_state_file_recurr, read_state_file,
};

async fn wait_job_instance_and_log(
  client: &NanocldClient,
  instance: &Process,
  query: &ProcessLogQuery,
) {
  let Ok(mut stream) = client.watch_events(None).await else {
    return;
  };
  while let Some(event) = stream.next().await {
    let Ok(event) = event else {
      continue;
    };
    if event.action != NativeEventAction::Start.to_string() {
      continue;
    };
    let Some(actor) = event.actor else {
      continue;
    };
    let Some(key) = actor.key else {
      continue;
    };
    if key != instance.name {
      continue;
    }
    match client.logs_process(&key, Some(query)).await {
      Err(err) => {
        eprintln!("Cannot get job instance {key} logs: {err}");
      }
      Ok(stream) => {
        if let Err(err) = utils::print::logs_process_stream(stream).await {
          eprintln!("{err}");
        }
      }
    }
    break;
  }
}

/// Logs existing jobs in the Statefile
async fn log_jobs(
  client: &NanocldClient,
  jobs: Vec<JobPartial>,
  query: &ProcessLogQuery,
) {
  // TODO: find a better way to wait for job process to start
  // sleep for 2 seconds for job process to start
  ntex::time::sleep(Duration::from_secs(2)).await;
  jobs
    .iter()
    .map(|job| async move {
      let job = match client.inspect_job(&job.name).await {
        Ok(job) => job,
        Err(err) => {
          eprintln!("Unable to inspect job {}: {err}", job.name);
          return;
        }
      };
      job
        .instances
        .iter()
        .map(|instance| async move {
          let started_at =
            instance.clone().data.state.unwrap_or_default().started_at;
          match started_at {
            None => {
              wait_job_instance_and_log(client, instance, query).await;
            }
            Some(started_at) => {
              if started_at == "0001-01-01T00:00:00Z" {
                wait_job_instance_and_log(client, instance, query).await;
                return;
              }
              let stream = client
                .logs_process(&instance.name, Some(query))
                .await
                .unwrap();
              if let Err(err) = utils::print::logs_process_stream(stream).await
              {
                eprintln!("{err}");
              }
            }
          }
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await;
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// Attach to a list of cargoes and print their logs
async fn log_cargoes(
  client: &NanocldClient,
  cargoes: Vec<CargoSpec>,
  namespace: &str,
  query: &ProcessLogQuery,
) {
  cargoes
    .into_iter()
    .map(|cargo| async move {
      let key = match utils::process::resource_key(&cargo.name, namespace) {
        Ok(key) => key,
        Err(err) => {
          eprintln!("Cannot resolve cargo {} key: {err}", cargo.name);
          return;
        }
      };
      match client.logs_processes("cargo", &key, Some(query)).await {
        Err(err) => {
          eprintln!("Cannot attach to cargo {}: {err}", cargo.name);
        }
        Ok(stream) => {
          if let Err(err) = utils::print::logs_process_stream(stream).await {
            eprintln!("{err}");
          }
        }
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

pub(super) async fn state_logs(
  cli_conf: &CliConfig,
  opts: &StateLogsOpts,
  state_file: &StateRef<Statefile>,
) {
  let client = &cli_conf.client;
  let tail = opts.tail.clone();
  let log_opts = ProcessLogQuery {
    since: opts.since,
    until: opts.until,
    tail,
    timestamps: Some(opts.timestamps),
    follow: Some(opts.follow),
    ..Default::default()
  };
  join!(
    log_jobs(
      client,
      state_file.data.jobs.clone().unwrap_or_default(),
      &log_opts
    ),
    log_cargoes(
      client,
      state_file.data.cargoes.clone().unwrap_or_default(),
      state_file.data.namespace.as_deref().unwrap_or("global"),
      &log_opts
    )
  );
}

/// Follow logs of all cargoes in state
pub(super) async fn exec_state_logs(
  cli_conf: &CliConfig,
  opts: &StateLogsOpts,
) -> IoResult<()> {
  let format = cli_conf.user_config.display_format.clone();
  let state_file = read_state_file(&opts.source, &format).await?;
  let args =
    parse_build_args(&state_file.data, ArgParseMode::Logs, &opts.args, false)?;
  let states =
    parse_state_file_recurr(cli_conf, &state_file, &args, false).await?;
  states
    .iter()
    .map(|state| state_logs(cli_conf, opts, state))
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
  Ok(())
}
