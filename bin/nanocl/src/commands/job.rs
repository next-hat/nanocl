use futures::StreamExt;
use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::stubs::{
  job::{JobHistoryQuery, JobInspect},
  process::{ProcessLogQuery, ProcessWaitQuery},
};

use crate::{
  config::CliConfig,
  models::{
    GenericDefaultOpts, GenericHistoryOpts, JobArg, JobCommand, JobCompactRow,
    JobLogsOpts, JobRevertOpts, JobRow, JobRunRow, JobWaitOpts, SpecHistoryRow,
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

impl GenericCommandStart for JobArg {}

impl GenericCommandStop for JobArg {}

impl GenericCommandInspect for JobArg {
  type ApiItem = JobInspect;
}

/// Show saved configuration revisions independently of live Jobs.
async fn exec_job_history(
  cli_conf: &CliConfig,
  opts: &GenericHistoryOpts,
) -> IoResult<()> {
  super::history::validate_options(opts)?;
  if let Some(id) = &opts.id {
    let history = cli_conf
      .client
      .inspect_history_job(&opts.key, &id.to_string())
      .await?;
    return super::history::print_detail(
      opts,
      &cli_conf.user_config.display_format,
      history,
    );
  }
  let revisions = cli_conf
    .client
    .list_history_job(
      &opts.key,
      Some(&JobHistoryQuery {
        limit: opts.limit,
        offset: opts.offset,
      }),
    )
    .await?;
  super::history::print_list(opts, revisions, SpecHistoryRow::from)
}

/// Restore a saved configuration without accepting a new Job execution.
async fn exec_job_revert(
  cli_conf: &CliConfig,
  opts: &JobRevertOpts,
) -> IoResult<()> {
  let job = cli_conf
    .client
    .revert_job(&opts.name, &opts.history_id.to_string())
    .await?;
  utils::print::print_yml(job)?;
  Ok(())
}

/// Show durable executions independently of live Jobs and Docker log retention.
async fn exec_job_runs(
  cli_conf: &CliConfig,
  opts: &GenericHistoryOpts,
) -> IoResult<()> {
  super::history::validate_options(opts)?;
  if let Some(id) = &opts.id {
    let run = cli_conf
      .client
      .inspect_run_job(&opts.key, &id.to_string())
      .await?;
    return super::history::print_detail(
      opts,
      &cli_conf.user_config.display_format,
      run,
    );
  }
  let runs = cli_conf
    .client
    .list_runs_job(
      &opts.key,
      Some(&JobHistoryQuery {
        limit: opts.limit,
        offset: opts.offset,
      }),
    )
    .await?;
  super::history::print_list(opts, runs, JobRunRow::from)
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
    JobCommand::Revert(opts) => exec_job_revert(cli_conf, opts).await,
    JobCommand::Runs(opts) => exec_job_runs(cli_conf, opts).await,
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
    ConnectOpts, NANOCLD_DEFAULT_VERSION, NanocldClient,
    stubs::job::{
      JobRun, JobRunInspect, JobRunOperation, JobRunState, JobRunStep, JobSpec,
      JobStepState,
    },
  };
  use ntex::web;

  use super::*;
  use crate::{
    config::UserConfig,
    models::{Context, DisplayFormat, GenericStartOpts, GenericStopOpts},
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
        .json(&serde_json::json!({"msg": "Retained entry was not found"}));
    }
    if request.path().ends_with("/histories") {
      return web::HttpResponse::Ok().json(&vec![fixture.history.spec.clone()]);
    }
    if request.path().contains("/histories/") {
      return web::HttpResponse::Ok().json(&fixture.history.spec);
    }
    if request.path().ends_with("/runs") {
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
            "/v{NANOCLD_DEFAULT_VERSION}/jobs/{{name}}/histories/{{id}}"
          ))
          .route(web::get().to(serve_history)),
        )
        .service(
          web::resource(format!(
            "/v{NANOCLD_DEFAULT_VERSION}/jobs/{{name}}/runs"
          ))
          .route(web::get().to(serve_history)),
        )
        .service(
          web::resource(format!(
            "/v{NANOCLD_DEFAULT_VERSION}/jobs/{{name}}/runs/{{run}}"
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
  async fn job_history_and_runs_list_pages_without_live_job_inspection() {
    let (cli, _server, fixture) = history_fixture();
    for command in ["history", "runs"] {
      let args = JobArg::try_parse_from([
        "job", command, "backup", "--limit", "2", "--offset", "1",
      ])
      .unwrap();
      exec_job(&cli, &args).await.unwrap();
    }
    assert_eq!(
      *fixture.requests.lock().unwrap(),
      vec![
        (
          format!("/v{NANOCLD_DEFAULT_VERSION}/jobs/backup/histories"),
          "limit=2&offset=1".into(),
        ),
        (
          format!("/v{NANOCLD_DEFAULT_VERSION}/jobs/backup/runs"),
          "limit=2&offset=1".into(),
        ),
      ]
    );
    let table = utils::print::render_table([SpecHistoryRow::from(
      fixture.history.spec.clone(),
    )]);
    for expected in [
      "ID",
      "VERSION",
      "CREATED AT",
      "v0.19",
      &fixture.history.spec.key.to_string(),
    ] {
      assert!(table.contains(expected), "{table}");
    }
    let table = utils::print::render_table([JobRunRow::from(
      fixture.history.run.clone(),
    )]);
    for expected in [
      "ID",
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
  async fn job_history_and_runs_limit_toml_to_detail() {
    let (cli, _server, fixture) = history_fixture();
    for command in ["history", "runs"] {
      let args =
        JobArg::try_parse_from(["job", command, "backup", "--display", "toml"])
          .unwrap();
      let error = exec_job(&cli, &args).await.unwrap_err();
      assert!(error.to_string().contains("TOML output requires --id"));
    }
    assert!(fixture.requests.lock().unwrap().is_empty());

    for (command, path, key) in [
      ("history", "histories", fixture.history.spec.key),
      ("runs", "runs", fixture.history.run.key),
    ] {
      let key = key.to_string();
      let args = JobArg::try_parse_from([
        "job",
        command,
        "backup",
        "--id",
        &key,
        "--display",
        "toml",
      ])
      .unwrap();
      exec_job(&cli, &args).await.unwrap();
      assert_eq!(
        fixture.requests.lock().unwrap().pop().unwrap(),
        (
          format!("/v{NANOCLD_DEFAULT_VERSION}/jobs/backup/{path}/{key}"),
          String::new(),
        )
      );
      assert!(fixture.requests.lock().unwrap().is_empty());
    }
  }

  #[ntex::test]
  async fn job_history_and_runs_format_detail_as_json_or_configured_yaml() {
    let (cli, _server, fixture) = history_fixture();
    assert!(matches!(
      cli.user_config.display_format,
      DisplayFormat::Yaml
    ));
    for (command, path, key) in [
      ("history", "histories", fixture.history.spec.key),
      ("runs", "runs", fixture.history.run.key),
    ] {
      let key = key.to_string();
      for display in [None, Some("json")] {
        let mut arguments =
          vec!["job", command, "backup", "--id", key.as_str()];
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
            format!("/v{NANOCLD_DEFAULT_VERSION}/jobs/backup/{path}/{key}"),
            String::new(),
          );
          2
        ]
      );
      fixture.requests.lock().unwrap().clear();
    }
  }

  #[ntex::test]
  async fn job_history_and_runs_propagate_errors() {
    let (cli, _server, fixture) = history_fixture();
    for command in ["history", "runs"] {
      let args = JobArg::try_parse_from([
        "job",
        command,
        "missing",
        "--id",
        "00000000-0000-0000-0000-000000000001",
      ])
      .unwrap();
      let error = exec_job(&cli, &args).await.unwrap_err();
      assert!(error.to_string().contains("Retained entry was not found"));
    }
    assert_eq!(fixture.requests.lock().unwrap().len(), 2);
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
  #[ntex::test]
  async fn job_state_lifecycle_uses_admission_without_status_or_events() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server_calls = calls.clone();
    let server = web::test::server(async move |_| {
      let calls = server_calls.clone();
      web::App::new().service(
        web::resource(format!(
          "/v{NANOCLD_DEFAULT_VERSION}/processes/job/{name}/{action}",
          name = "{name}",
          action = "{action}"
        ))
        .route(web::post().to(move |request: web::HttpRequest| {
          let calls = calls.clone();
          async move {
            calls.lock().unwrap().push(request.path().to_owned());
            if request.match_info().get("name") == Some("rejected") {
              web::HttpResponse::Conflict()
                .json(&serde_json::json!({"msg": "active lifecycle"}))
            } else {
              web::HttpResponse::Accepted().body("")
            }
          }
        })),
      )
    });
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: format!("http://localhost:{}", server.addr().port()),
      ..Default::default()
    })
    .unwrap();
    for name in ["backup", "rejected"] {
      let expected = usize::from(name == "rejected");
      let keys = vec![name.to_owned()];
      assert_eq!(
        JobArg::exec_start_with_progress(
          &client,
          &GenericStartOpts { keys: keys.clone() },
          None,
          None
        )
        .await
        .unwrap(),
        expected
      );
      assert_eq!(
        JobArg::exec_stop_with_progress(
          &client,
          &GenericStopOpts { keys },
          None,
          None
        )
        .await
        .unwrap(),
        expected
      );
    }
    assert_eq!(calls.lock().unwrap().len(), 4);
  }
}
