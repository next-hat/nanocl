use futures::StreamExt;
use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::{
  NanocldClient,
  stubs::{
    job::{JobHistoryQuery, JobInspect},
    process::{ProcessLogQuery, ProcessWaitQuery},
  },
};

use crate::{
  config::CliConfig,
  models::{
    GenericDefaultOpts, GenericStartOpts, GenericStopOpts, JobArg, JobCommand,
    JobCompactRow, JobHistoryOpts, JobLogsOpts, JobRow, JobRunRow, JobWaitOpts,
  },
  utils,
};

use super::{
  GenericCommand, GenericCommandInspect, GenericCommandLs, GenericCommandRm,
  GenericCommandStart, GenericCommandStop,
};

impl GenericCommand for JobArg {
  fn object_name() -> &'static str {
    "jobs"
  }
}

impl GenericCommandLs for JobArg {
  type Item = JobRow;
  type CompactItem = JobCompactRow;
  type Args = JobArg;
  type ApiItem = nanocld_client::stubs::job::JobSummary;

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

impl GenericCommandRm<GenericDefaultOpts, String> for JobArg {}

impl GenericCommandStart for JobArg {
  async fn exec_start(
    client: &NanocldClient,
    opts: &GenericStartOpts,
  ) -> IoResult<()> {
    // Admission is authoritative; legacy current status cannot establish
    // whether a retained run is active. Do not wait on name-only events.
    for key in &opts.keys {
      client.start_process("job", key).await?;
    }
    Ok(())
  }
}

impl GenericCommandStop for JobArg {
  async fn exec_stop(
    client: &NanocldClient,
    opts: &GenericStopOpts,
  ) -> IoResult<()> {
    for key in &opts.keys {
      client.stop_process("job", key).await?;
    }
    Ok(())
  }
}

impl GenericCommandInspect for JobArg {
  type ApiItem = JobInspect;
}

/// Show durable run history independently of live Jobs and Docker log retention.
async fn exec_job_history(
  cli_conf: &CliConfig,
  opts: &JobHistoryOpts,
) -> IoResult<()> {
  if let Some(run) = &opts.run {
    let history = cli_conf.client.inspect_history_job(&opts.name, run).await?;
    let display = opts
      .display
      .as_ref()
      .unwrap_or(&cli_conf.user_config.display_format);
    return utils::print::display_format(display, history);
  }
  if matches!(
    opts.display.as_ref(),
    Some(crate::models::DisplayFormat::Toml)
  ) {
    return Err(IoError::other(
      "Job history",
      "TOML output requires --run RUN-ID; use yaml or json for a history list",
    ));
  }
  let runs = cli_conf
    .client
    .list_history_job(
      &opts.name,
      Some(&JobHistoryQuery {
        limit: opts.limit,
        offset: opts.offset,
      }),
    )
    .await?;
  if let Some(display) = &opts.display {
    return utils::print::display_format(display, runs);
  }
  let output =
    utils::print::render_table(runs.into_iter().map(JobRunRow::from));
  if !output.is_empty() {
    println!("{output}");
  }
  Ok(())
}

/// Execute the `nanocl job logs` command to list the logs of a job
async fn exec_job_logs(
  cli_conf: &CliConfig,
  opts: &JobLogsOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let query = ProcessLogQuery {
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
      JobArg::exec_ls(&cli_conf.client, args, opts, None).await
    }
    JobCommand::Remove(opts) => JobArg::exec_rm(&cli_conf.client, opts).await,
    JobCommand::Inspect(opts) => JobArg::exec_inspect(cli_conf, opts).await,
    JobCommand::History(opts) => exec_job_history(cli_conf, opts).await,
    JobCommand::Logs(opts) => exec_job_logs(cli_conf, opts).await,
    JobCommand::Wait(opts) => exec_job_wait(cli_conf, opts).await,
    JobCommand::Start(opts) => JobArg::exec_start(&cli_conf.client, opts).await,
    JobCommand::Stop(opts) => JobArg::exec_stop(&cli_conf.client, opts).await,
  }
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use clap::Parser;
  use nanocld_client::{
    ConnectOpts, NANOCLD_DEFAULT_VERSION,
    stubs::job::{
      JobRun, JobRunInspect, JobRunOperation, JobRunState, JobRunStep, JobSpec,
      JobStepState,
    },
  };
  use ntex::web;

  use super::*;
  use crate::{
    config::UserConfig,
    models::{Context, DisplayFormat},
  };

  struct HistoryFixture {
    requests: Mutex<Vec<(String, String)>>,
    history: JobRunInspect,
  }

  async fn serve_history(request: web::HttpRequest) -> web::HttpResponse {
    let fixture = request.app_state::<Arc<HistoryFixture>>().unwrap();
    fixture
      .requests
      .lock()
      .unwrap()
      .push((request.path().to_owned(), request.query_string().to_owned()));
    if request.path().contains("/jobs/missing/") {
      return web::HttpResponse::NotFound()
        .json(&serde_json::json!({"msg": "Retained execution was not found"}));
    }
    if request.path().ends_with("/histories") {
      return web::HttpResponse::Ok().json(&vec![fixture.history.run.clone()]);
    }
    web::HttpResponse::Ok().json(&fixture.history)
  }

  fn history_fixture() -> (CliConfig, web::test::TestServer, Arc<HistoryFixture>)
  {
    let run_key = uuid::Uuid::from_u128(1);
    let incarnation_key = uuid::Uuid::from_u128(2);
    let spec_key = uuid::Uuid::from_u128(3);
    let operation_key = uuid::Uuid::from_u128(4);
    let time = chrono::DateTime::UNIX_EPOCH.naive_utc();
    let fixture = Arc::new(HistoryFixture {
      requests: Mutex::new(Vec::new()),
      history: JobRunInspect {
        run: JobRun {
          key: run_key,
          job_name: "backup".into(),
          incarnation_key,
          spec_key,
          operation_key,
          accepted_at: time,
          started_at: Some(time),
          finished_at: Some(time),
          state: JobRunState::Succeeded,
          reason: None,
        },
        spec: JobSpec {
          key: spec_key,
          job_key: "backup".into(),
          version: "v0.19".into(),
          created_at: time,
          name: "backup".into(),
          ..Default::default()
        },
        operation: JobRunOperation {
          key: operation_key,
          incarnation_key,
          resource_kind: "Job".into(),
          resource_key: "backup".into(),
          action: "start".into(),
          spec_key,
          accepted_at: time,
          completed_at: Some(time),
          outcome: Some("completed".into()),
          diagnostic: None,
        },
        steps: vec![JobRunStep {
          run_key,
          step_index: 0,
          container_id: Some("immutable-container-id".into()),
          state: JobStepState::Succeeded,
          started_at: Some(time),
          finished_at: Some(time),
          exit_code: Some(0),
          error: None,
        }],
      },
    });
    let server_fixture = fixture.clone();
    let server = web::test::server(async move |_| {
      web::App::new()
        .with_config(web::WebAppConfig::new().set_state(server_fixture.clone()))
        .service(
          web::resource(format!(
            "/v{NANOCLD_DEFAULT_VERSION}/jobs/{{name}}/histories"
          ))
          .route(web::get().to(serve_history)),
        )
        .service(
          web::resource(format!(
            "/v{NANOCLD_DEFAULT_VERSION}/jobs/{{name}}/histories/{{run}}"
          ))
          .route(web::get().to(serve_history)),
        )
    });
    let host = format!("http://localhost:{}", server.addr().port());
    let cli = CliConfig {
      client: NanocldClient::connect_to(&ConnectOpts {
        url: host.clone(),
        ..Default::default()
      })
      .unwrap(),
      host,
      context: Context::default(),
      user_config: UserConfig::default(),
    };
    (cli, server, fixture)
  }

  #[ntex::test]
  async fn job_history_command_lists_pages_without_live_job_inspection() {
    let (cli, _server, fixture) = history_fixture();
    let args = JobArg::try_parse_from([
      "job", "history", "backup", "--limit", "2", "--offset", "1",
    ])
    .unwrap();
    exec_job(&cli, &args).await.unwrap();
    assert_eq!(
      *fixture.requests.lock().unwrap(),
      vec![(
        format!("/v{NANOCLD_DEFAULT_VERSION}/jobs/backup/histories"),
        "limit=2&offset=1".into(),
      )]
    );
    let table = utils::print::render_table([JobRunRow::from(
      fixture.history.run.clone(),
    )]);
    for expected in [
      "RUN",
      "INCARNATION",
      "STATE",
      "Succeeded",
      &fixture.history.run.key.to_string(),
      &fixture.history.run.incarnation_key.to_string(),
    ] {
      assert!(table.contains(expected), "{table}");
    }
  }

  #[ntex::test]
  async fn job_history_command_limits_toml_to_run_detail() {
    let (cli, _server, fixture) = history_fixture();
    let args =
      JobArg::try_parse_from(["job", "history", "backup", "--display", "toml"])
        .unwrap();
    let error = exec_job(&cli, &args).await.unwrap_err();
    assert!(error.to_string().contains("TOML output requires --run"));
    assert!(fixture.requests.lock().unwrap().is_empty());

    let key = fixture.history.run.key.to_string();
    let args = JobArg::try_parse_from([
      "job",
      "history",
      "backup",
      "--run",
      &key,
      "--display",
      "toml",
    ])
    .unwrap();
    exec_job(&cli, &args).await.unwrap();
    assert_eq!(
      *fixture.requests.lock().unwrap(),
      vec![(
        format!("/v{NANOCLD_DEFAULT_VERSION}/jobs/backup/histories/{key}"),
        String::new(),
      )]
    );
  }

  #[ntex::test]
  async fn job_history_command_formats_detail_as_json_or_configured_yaml() {
    let (cli, _server, fixture) = history_fixture();
    assert!(matches!(
      cli.user_config.display_format,
      DisplayFormat::Yaml
    ));
    let key = fixture.history.run.key.to_string();
    for display in [None, Some("json")] {
      let mut arguments =
        vec!["job", "history", "backup", "--run", key.as_str()];
      if let Some(display) = display {
        arguments.extend(["--display", display]);
      }
      exec_job(&cli, &JobArg::try_parse_from(arguments).unwrap())
        .await
        .unwrap();
    }
    assert_eq!(
      *fixture.requests.lock().unwrap(),
      vec![
        (
          format!("/v{NANOCLD_DEFAULT_VERSION}/jobs/backup/histories/{key}"),
          String::new(),
        );
        2
      ]
    );
  }

  #[ntex::test]
  async fn job_history_command_propagates_history_errors() {
    let (cli, _server, fixture) = history_fixture();
    let args = JobArg::try_parse_from([
      "job",
      "history",
      "missing",
      "--run",
      "00000000-0000-0000-0000-000000000001",
    ])
    .unwrap();
    let error = exec_job(&cli, &args).await.unwrap_err();
    assert!(
      error
        .to_string()
        .contains("Retained execution was not found")
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
  }

  struct RemovalFixture {
    job: JobInspect,
    query: Mutex<Option<String>>,
  }

  async fn inspect_removal_fixture(
    request: web::HttpRequest,
  ) -> web::HttpResponse {
    web::HttpResponse::Ok()
      .json(&request.app_state::<Arc<RemovalFixture>>().unwrap().job)
  }

  async fn reject_changed_removal(
    request: web::HttpRequest,
  ) -> web::HttpResponse {
    *request
      .app_state::<Arc<RemovalFixture>>()
      .unwrap()
      .query
      .lock()
      .unwrap() = Some(request.query_string().to_owned());
    web::HttpResponse::Conflict()
      .json(&serde_json::json!({"msg": "Job run changed before deletion"}))
  }

  #[ntex::test]
  async fn job_remove_fences_inspected_run_and_propagates_conflict() {
    let incarnation_key = uuid::Uuid::from_u128(1);
    let run_key = uuid::Uuid::from_u128(2);
    let time = chrono::DateTime::UNIX_EPOCH.naive_utc();
    let fixture = Arc::new(RemovalFixture {
      query: Mutex::new(None),
      job: JobInspect {
        incarnation_key,
        spec_key: uuid::Uuid::from_u128(3),
        current_run_key: Some(run_key),
        created_at: time,
        updated_at: time,
        status: Default::default(),
        instance_total: 0,
        instance_success: 0,
        instance_running: 0,
        instance_failed: 0,
        spec: JobSpec {
          key: uuid::Uuid::from_u128(3),
          job_key: "backup".into(),
          version: "v0.19".into(),
          created_at: time,
          name: "backup".into(),
          ..Default::default()
        },
        instances: vec![],
      },
    });
    let server_fixture = fixture.clone();
    let server = web::test::server(async move |_| {
      web::App::new()
        .with_config(web::WebAppConfig::new().set_state(server_fixture.clone()))
        .service(
          web::resource(format!(
            "/v{NANOCLD_DEFAULT_VERSION}/jobs/backup/inspect"
          ))
          .route(web::get().to(inspect_removal_fixture)),
        )
        .service(
          web::resource(format!("/v{NANOCLD_DEFAULT_VERSION}/events/watch"))
            .route(web::post().to(|| async {
              web::HttpResponse::Ok()
                .content_type("application/vdn.nanocl.raw-stream")
                .body("")
            })),
        )
        .service(
          web::resource(format!("/v{NANOCLD_DEFAULT_VERSION}/jobs/backup"))
            .route(web::delete().to(reject_changed_removal)),
        )
    });
    let host = format!("http://localhost:{}", server.addr().port());
    let cli = CliConfig {
      client: NanocldClient::connect_to(&ConnectOpts {
        url: host.clone(),
        ..Default::default()
      })
      .unwrap(),
      host,
      context: Context::default(),
      user_config: UserConfig::default(),
    };
    let args =
      JobArg::try_parse_from(["job", "remove", "-y", "backup"]).unwrap();
    let error = tokio::time::timeout(
      std::time::Duration::from_secs(5),
      exec_job(&cli, &args),
    )
    .await
    .expect("Job removal waited for an unaccepted deletion")
    .unwrap_err();
    assert!(
      error
        .to_string()
        .contains("Job run changed before deletion")
    );
    assert_eq!(
      *fixture.query.lock().unwrap(),
      Some(format!("incarnation={incarnation_key}&run={run_key}"))
    );
  }
}
