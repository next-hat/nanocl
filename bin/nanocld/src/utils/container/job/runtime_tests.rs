//! Real admission/persistence with a task-owned mock Docker HTTP transport.
//! The separately ignored, explicitly opted-in Docker test uses fresh containers;
//! the default tests make no host Docker requests or service restarts.
use std::sync::{
  Arc, Mutex,
  atomic::{AtomicBool, Ordering},
};

use diesel::prelude::*;
use nanocl_stubs::{
  generic::ImagePullPolicy,
  job::{JobPartial, JobRunState},
};
use ntex::web;
use serde_json::json;

use super::*;
use crate::{
  models::{NodeDb, OperationDb, SystemStateInner},
  repositories::operation::{run_query_owned, transaction},
  tasks::generic::ObjTask,
  utils::tests::{TestSystem, gen_test_system},
};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
  Success,
  BlockFirst,
  BlockSecond,
  BlockStop,
  Nonzero,
  StartError,
  WaitError,
  ProcessSyncError,
  CreateError(u16),
  StopAlreadyStopped,
  StopUnconfirmed,
}

#[derive(Clone)]
struct Runtime {
  id: String,
  config: Config,
  running: bool,
  started: bool,
  exit: Option<i64>,
}

impl Runtime {
  fn inspect(&self) -> serde_json::Value {
    json!({"Id":self.id,"Name":format!("/{}.j",self.id),"Created":"2026-10-27T18:00:00Z",
      "Config":self.config,"State":{"Running":self.running,"Restarting":false,
        "StartedAt":if self.started {"2026-10-27T18:00:01Z"} else {"0001-01-01T00:00:00Z"},
        "FinishedAt":if self.exit.is_some() {"2026-10-27T18:00:02Z"} else {"0001-01-01T00:00:00Z"},
        "ExitCode":self.exit.unwrap_or(0)}})
  }
}

#[derive(Clone)]
struct DockerFixture {
  mode: Mode,
  runtimes: Arc<Mutex<Vec<Runtime>>>,
  requests: Arc<Mutex<Vec<String>>>,
  release: Arc<AtomicBool>,
  wait_entered: Arc<AtomicBool>,
  wake: Arc<tokio::sync::Notify>,
}

async fn create(
  request: web::HttpRequest,
  body: web::types::Json<Config>,
) -> web::HttpResponse {
  let fixture = request.app_state::<DockerFixture>().unwrap();
  fixture.requests.lock().unwrap().push("create".into());
  if let Mode::CreateError(status) = fixture.mode {
    return web::HttpResponse::builder(
      ntex::http::StatusCode::from_u16(status).unwrap(),
    )
    .json(&json!({"message":"secret-value-must-not-persist"}));
  }
  let mut runtimes = fixture.runtimes.lock().unwrap();
  let id = format!("job-mock-{}", uuid::Uuid::new_v4().simple());
  runtimes.push(Runtime {
    id: id.clone(),
    config: body.into_inner(),
    running: false,
    started: false,
    exit: None,
  });
  web::HttpResponse::Created().json(&json!({"Id":id,"Warnings":[]}))
}

async fn request(request: web::HttpRequest) -> web::HttpResponse {
  let fixture = request.app_state::<DockerFixture>().unwrap().clone();
  let path = request.path();
  fixture
    .requests
    .lock()
    .unwrap()
    .push(format!("{} {path}", request.method()));
  if path.ends_with("/containers/json") {
    let filters =
      url::form_urlencoded::parse(request.query_string().as_bytes())
        .find(|(key, _)| key == "filters")
        .map(|(_, value)| {
          serde_json::from_str::<HashMap<String, Vec<String>>>(&value).unwrap()
        })
        .unwrap_or_default();
    let labels = filters.get("label").cloned().unwrap_or_default();
    let runtimes = fixture.runtimes.lock().unwrap();
    return web::HttpResponse::Ok().json(
      &runtimes
        .iter()
        .filter(|runtime| {
          labels.iter().all(|filter| {
            let (key, value) = filter.split_once('=').unwrap();
            runtime
              .config
              .labels
              .as_ref()
              .and_then(|labels| labels.get(key))
              .is_some_and(|observed| observed == value)
          })
        })
        .map(|runtime| json!({"Id":runtime.id,"Labels":runtime.config.labels}))
        .collect::<Vec<_>>(),
    );
  }
  let Some(rest) = path.split("/containers/").nth(1) else {
    return web::HttpResponse::NotFound()
      .json(&json!({"message":"Unexpected mock request"}));
  };
  let id = rest.split('/').next().unwrap();
  if path.ends_with("/start") {
    if fixture.mode == Mode::StartError {
      return web::HttpResponse::InternalServerError()
        .json(&json!({"message":"secret-value-must-not-persist"}));
    }
    let mut runtimes = fixture.runtimes.lock().unwrap();
    let runtime = runtimes
      .iter_mut()
      .find(|runtime| runtime.id == id)
      .unwrap();
    runtime.running = true;
    runtime.started = true;
    return web::HttpResponse::NoContent().build();
  }
  if path.ends_with("/stop") {
    if fixture.mode == Mode::BlockStop {
      loop {
        let wake = fixture.wake.notified();
        if fixture.release.load(Ordering::SeqCst) {
          break;
        }
        wake.await;
      }
    }
    if fixture.mode == Mode::StopUnconfirmed {
      return web::HttpResponse::NotModified().build();
    }
    let mut runtimes = fixture.runtimes.lock().unwrap();
    let runtime = runtimes
      .iter_mut()
      .find(|runtime| runtime.id == id)
      .unwrap();
    runtime.running = false;
    if runtime.started {
      runtime.exit = Some(if fixture.mode == Mode::StopAlreadyStopped {
        0
      } else {
        137
      });
    }
    fixture.release.store(true, Ordering::SeqCst);
    fixture.wake.notify_waiters();
    if fixture.mode == Mode::StopAlreadyStopped {
      return web::HttpResponse::NotModified().build();
    }
    return web::HttpResponse::NoContent().build();
  }
  if path.ends_with("/wait") {
    if fixture.mode == Mode::WaitError {
      return web::HttpResponse::InternalServerError()
        .json(&json!({"message":"secret-value-must-not-persist"}));
    }
    let index = {
      fixture
        .runtimes
        .lock()
        .unwrap()
        .iter()
        .find(|runtime| runtime.id == id)
        .unwrap()
        .config
        .labels
        .as_ref()
        .unwrap()[STEP_LABEL]
        .parse::<i32>()
        .unwrap()
    };
    let blocked = matches!(
      fixture.mode,
      Mode::BlockFirst
        | Mode::BlockStop
        | Mode::StopAlreadyStopped
        | Mode::StopUnconfirmed
    ) || (fixture.mode == Mode::BlockSecond && index == 1);
    if blocked {
      fixture.wait_entered.store(true, Ordering::SeqCst);
      loop {
        let wake = fixture.wake.notified();
        if fixture.release.load(Ordering::SeqCst) {
          break;
        }
        wake.await;
      }
    }
    let mut runtimes = fixture.runtimes.lock().unwrap();
    let runtime = runtimes
      .iter_mut()
      .find(|runtime| runtime.id == id)
      .unwrap();
    runtime.running = false;
    let code = *runtime
      .exit
      .get_or_insert(if fixture.mode == Mode::Nonzero { 7 } else { 0 });
    return web::HttpResponse::Ok().json(&json!({"StatusCode":code}));
  }
  if request.method() == ntex::http::Method::DELETE {
    fixture
      .runtimes
      .lock()
      .unwrap()
      .retain(|runtime| runtime.id != id);
    return web::HttpResponse::NoContent().build();
  }
  if path.ends_with("/json") {
    let runtimes = fixture.runtimes.lock().unwrap();
    return match runtimes.iter().find(|runtime| runtime.id == id) {
      Some(runtime) => {
        let mut response = runtime.inspect();
        if fixture.mode == Mode::ProcessSyncError {
          // Docker state remains valid, but Process snapshot conversion fails.
          response["Created"] = json!("invalid-process-created-timestamp");
        }
        web::HttpResponse::Ok().json(&response)
      }
      None => web::HttpResponse::NotFound()
        .json(&json!({"message":"No such container"})),
    };
  }
  web::HttpResponse::NotFound()
    .json(&json!({"message":"Unexpected mock request"}))
}

struct Fixture {
  system: TestSystem,
  state: SystemState,
  job: Job,
  docker: DockerFixture,
  _server: web::test::TestServer,
}

impl Fixture {
  async fn new(mode: Mode) -> Self {
    Self::with_image(mode, "job-fixture:latest", false).await
  }

  async fn with_image(mode: Mode, image: &str, real_commands: bool) -> Self {
    fn no_routes(_: &mut web::ServiceConfig<SystemState>) {}
    let system = gen_test_system(no_routes, crate::vars::VERSION).await;
    let docker = DockerFixture {
      mode,
      runtimes: Default::default(),
      requests: Default::default(),
      release: Default::default(),
      wait_entered: Default::default(),
      wake: Default::default(),
    };
    let server_fixture = docker.clone();
    let server = web::test::server(async move |_| {
      web::App::new()
        .with_config(web::WebAppConfig::new().set_state(server_fixture.clone()))
        .service(
          web::resource("/{version}/containers/create")
            .route(web::post().to(create)),
        )
        .default_service(web::route().to(request))
    });
    let mut config = system.state.inner.config.clone();
    config.hostname = format!("job-mock-{}", uuid::Uuid::new_v4().simple());
    config.state_dir = format!("{}/{}", config.state_dir, config.hostname);
    std::fs::create_dir_all(&config.state_dir).unwrap();
    let state = SystemState {
      inner: Arc::new(SystemStateInner {
        pool: system.state.inner.pool.clone(),
        docker_api: bollard_next::Docker::connect_with_http(
          &format!("http://{}", server.addr()),
          5,
          bollard_next::API_DEFAULT_VERSION,
        )
        .unwrap(),
        config,
        task_manager: Default::default(),
        event_emitter: system.state.inner.event_emitter.clone(),
        event_emitter_raw: Default::default(),
        arbiter: ntex::rt::Arbiter::current(),
      }),
    };
    NodeDb::create_from(
      NodeDb {
        name: state.inner.config.hostname.clone(),
        created_at: chrono::Utc::now().naive_utc(),
        endpoint: "mock-only".into(),
        version: crate::vars::VERSION.into(),
        metadata: None,
      },
      &state.inner.pool,
    )
    .await
    .unwrap();
    let container = Config {
      image: Some(image.into()),
      cmd: real_commands.then(|| {
        vec!["sh".into(), "-c".into(), "printf job-run-step\\n".into()]
      }),
      host_config: Some(HostConfig {
        network_mode: Some("none".into()),
        nano_cpus: real_commands.then_some(100_000_000),
        ..Default::default()
      }),
      ..Default::default()
    };
    let job = JobDb::create_with_spec(
      &JobPartial {
        name: format!("job-runtime-test-{}", uuid::Uuid::new_v4().simple()),
        image_pull_policy: Some(ImagePullPolicy::Never),
        containers: vec![container.clone(), container],
        ..Default::default()
      },
      "v0.19",
      &state.inner.pool,
    )
    .await
    .unwrap();
    Self {
      system,
      state,
      job,
      docker,
      _server: server,
    }
  }

  async fn current(&self) -> JobRunDb {
    let job =
      JobDb::transform_read_by_pk(&self.job.spec.name, &self.state.inner.pool)
        .await
        .unwrap();
    JobRunDb::get(job.current_run_key.unwrap(), &self.state.inner.pool)
      .await
      .unwrap()
  }

  async fn wait_state(&self, state: &str) -> JobRunDb {
    for _ in 0..1500 {
      let run = self.current().await;
      if run.state == state {
        if run.finished_at.is_some() {
          let _settled = lifecycle_lock(&self.job.spec.name).lock_owned().await;
        }
        return run;
      }
      ntex::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("Job never reached {state}: {:?}", self.current().await);
  }

  async fn wait_blocked(&self) {
    for _ in 0..1500 {
      if self.docker.wait_entered.load(Ordering::SeqCst) {
        return;
      }
      ntex::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("Docker wait was never reached: {:?}", self.current().await);
  }

  async fn accept(&self, complete: bool) -> (AcceptedJobRun, OperationGuard) {
    let job =
      JobDb::transform_read_by_pk(&self.job.spec.name, &self.state.inner.pool)
        .await
        .unwrap();
    let mutex = JobDb::create_mutex(&job.spec.name, "start", &self.state)
      .await
      .unwrap();
    let (owner, guard) = utils::operation::create_owner(
      mutex.key,
      "Job",
      &job.spec.name,
      job.incarnation_key,
      &self.state.inner.config.hostname,
      &self.state.inner.config.state_dir,
    )
    .unwrap();
    let op =
      OperationDb::new(&owner, "Job", &job.spec.name, job.spec_key, "start");
    let accepted =
      JobDb::accept_run(&job, op, guard.clone(), &self.state.inner.pool)
        .await
        .unwrap();
    if complete {
      JobDb::complete_start(
        &accepted.operation,
        guard.clone(),
        &self.state.inner.pool,
      )
      .await
      .unwrap();
    }
    (accepted, guard)
  }

  async fn cleanup(self) {
    // Let the exact control finish after run settlement before deleting fixtures.
    for _ in 0..1500 {
      if OperationDb::current(
        "Job",
        &self.job.spec.name,
        &self.state.inner.pool,
      )
      .await
      .unwrap()
      .is_none_or(|operation| operation.outcome.is_some())
      {
        break;
      }
      ntex::time::sleep(Duration::from_millis(20)).await;
    }
    let _serialized = lifecycle_lock(&self.job.spec.name).lock_owned().await;
    self.system.state.wait_event_loop().await;
    let name = self.job.spec.name.clone();
    let node = self.state.inner.config.hostname.clone();
    run_query_owned(&self.state.inner.pool, (), move |conn| {
      transaction(conn, |conn| {
        use crate::schema::{
          distributed_mutexes, job_run_steps, job_runs, jobs, nodes,
          object_process_statuses, operations, processes,
          resource_incarnations, specs,
        };
        let incarnations = resource_incarnations::table
          .filter(resource_incarnations::resource_kind.eq("Job"))
          .filter(resource_incarnations::resource_key.eq(&name))
          .select(resource_incarnations::key)
          .load::<uuid::Uuid>(conn)?;
        let runs = job_runs::table
          .filter(job_runs::incarnation_key.eq_any(&incarnations))
          .select(job_runs::key)
          .load::<uuid::Uuid>(conn)?;
        diesel::delete(processes::table.filter(processes::node_name.eq(&node)))
          .execute(conn)?;
        diesel::delete(jobs::table.find(&name)).execute(conn)?;
        diesel::delete(object_process_statuses::table.find(&name))
          .execute(conn)?;
        diesel::delete(
          job_run_steps::table.filter(job_run_steps::run_key.eq_any(&runs)),
        )
        .execute(conn)?;
        diesel::delete(job_runs::table.filter(job_runs::key.eq_any(&runs)))
          .execute(conn)?;
        diesel::update(
          resource_incarnations::table
            .filter(resource_incarnations::key.eq_any(&incarnations)),
        )
        .set(
          resource_incarnations::current_operation_key.eq(None::<uuid::Uuid>),
        )
        .execute(conn)?;
        diesel::delete(
          operations::table
            .filter(operations::incarnation_key.eq_any(&incarnations)),
        )
        .execute(conn)?;
        diesel::delete(
          specs::table.filter(specs::incarnation_key.eq_any(&incarnations)),
        )
        .execute(conn)?;
        diesel::delete(
          resource_incarnations::table
            .filter(resource_incarnations::key.eq_any(&incarnations)),
        )
        .execute(conn)?;
        diesel::delete(
          distributed_mutexes::table
            .filter(distributed_mutexes::node_key.eq(&node)),
        )
        .execute(conn)?;
        diesel::delete(nodes::table.find(&node)).execute(conn)?;
        Ok(())
      })
    })
    .await
    .unwrap();
    let _ = std::fs::remove_dir_all(&self.state.inner.config.state_dir);
  }
}

#[ntex::test]
async fn job_runtime_sequential_success_does_not_finish_after_first_step() {
  let fixture = Fixture::new(Mode::BlockSecond).await;
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  fixture.wait_blocked().await;
  let run = fixture.current().await;
  assert_eq!(run.state, "Running");
  assert!(run.finished_at.is_none());
  let steps = JobRunDb::steps(run.key, &fixture.state.inner.pool)
    .await
    .unwrap();
  assert_eq!(steps[0].exit_code, Some(0));
  assert_eq!(steps[1].exit_code, None);
  fixture.docker.release.store(true, Ordering::SeqCst);
  fixture.docker.wake.notify_waiters();
  let run = fixture.wait_state("Succeeded").await;
  let detail = JobDb::inspect_run(
    &fixture.job.spec.name,
    run.key,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  assert_eq!(detail.run.state, JobRunState::Succeeded);
  assert!(detail.steps.iter().all(|step| step.exit_code == Some(0)));
  assert_ne!(detail.steps[0].container_id, detail.steps[1].container_id);
  fixture.cleanup().await;
}

#[ntex::test]
async fn job_runtime_process_sync_failure_keeps_exit_observer() {
  let fixture = Fixture::new(Mode::ProcessSyncError).await;
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  let run = fixture.wait_state("Succeeded").await;
  let steps = JobRunDb::steps(run.key, &fixture.state.inner.pool)
    .await
    .unwrap();
  assert_eq!(steps.len(), 2);
  for step in steps {
    assert_eq!(step.state, "Succeeded");
    assert_eq!(step.exit_code, Some(0));
    let id = step.container_id.unwrap();
    assert!(
      ProcessDb::transform_read_by_pk(&id, &fixture.state.inner.pool)
        .await
        .is_err()
    );
  }
  assert_eq!(
    fixture
      .docker
      .requests
      .lock()
      .unwrap()
      .iter()
      .filter(|request| request.ends_with("/wait"))
      .count(),
    2
  );
  fixture.cleanup().await;
}

#[ntex::test]
async fn job_runtime_create_rejection_fails_without_inventing_execution() {
  for status in [400, 404, 409, 500] {
    let fixture = Fixture::new(Mode::CreateError(status)).await;
    crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
      .await
      .unwrap();
    let definitive = matches!(status, 400 | 404 | 409);
    let run = fixture
      .wait_state(if definitive { "Failed" } else { "Unknown" })
      .await;
    let steps = JobRunDb::steps(run.key, &fixture.state.inner.pool)
      .await
      .unwrap();
    assert_eq!(run.finished_at.is_some(), definitive);
    assert!(run.started_at.is_none());
    assert_eq!(
      steps[0].state,
      if definitive { "Failed" } else { "Unknown" }
    );
    assert_eq!(steps[0].finished_at.is_some(), definitive);
    assert!(steps.iter().all(|step| step.container_id.is_none()
      && step.exit_code.is_none()
      && step.started_at.is_none()));
    assert!(
      steps[0]
        .error
        .as_ref()
        .unwrap()
        .contains(&format!("Docker HTTP {status}"))
    );
    assert!(!steps[0].error.as_ref().unwrap().contains("secret-value"));
    assert_eq!(
      fixture
        .docker
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| *request == "create")
        .count(),
      1
    );
    assert!(fixture.docker.runtimes.lock().unwrap().is_empty());
    if !definitive {
      {
        let _settled =
          lifecycle_lock(&fixture.job.spec.name).lock_owned().await;
      }
      assert!(
        crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
          .await
          .is_err()
      );
      crate::tasks::job::stop(&fixture.job.spec.name, &fixture.state)
        .await
        .unwrap();
      fixture.wait_state("Interrupted").await;
    }
    fixture.cleanup().await;
  }
}

#[ntex::test]
async fn job_runtime_stop_already_stopped_requires_fresh_confirmation() {
  for mode in [Mode::StopAlreadyStopped, Mode::StopUnconfirmed] {
    let fixture = Fixture::new(mode).await;
    crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
      .await
      .unwrap();
    fixture.wait_blocked().await;
    crate::tasks::job::stop(&fixture.job.spec.name, &fixture.state)
      .await
      .unwrap();
    let confirmed = mode == Mode::StopAlreadyStopped;
    let run = fixture
      .wait_state(if confirmed { "Interrupted" } else { "Unknown" })
      .await;
    {
      let _settled = lifecycle_lock(&fixture.job.spec.name).lock_owned().await;
    }
    let steps = JobRunDb::steps(run.key, &fixture.state.inner.pool)
      .await
      .unwrap();
    assert_eq!(run.finished_at.is_some(), confirmed);
    assert_eq!(steps[0].exit_code, confirmed.then_some(0));
    assert!(steps[1].container_id.is_none());
    assert!(
      fixture
        .docker
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|request| request.ends_with("/stop"))
    );
    if !confirmed {
      assert!(
        crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
          .await
          .is_err()
      );
      {
        let mut runtimes = fixture.docker.runtimes.lock().unwrap();
        runtimes[0].running = false;
        runtimes[0].exit = Some(0);
      }
      crate::tasks::job::stop(&fixture.job.spec.name, &fixture.state)
        .await
        .unwrap();
      fixture.wait_state("Interrupted").await;
      fixture.docker.release.store(true, Ordering::SeqCst);
      fixture.docker.wake.notify_waiters();
    }
    fixture.cleanup().await;
  }
}

#[ntex::test]
async fn job_runtime_nonzero_and_start_errors_prevent_later_steps() {
  for mode in [Mode::Nonzero, Mode::StartError] {
    let fixture = Fixture::new(mode).await;
    crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
      .await
      .unwrap();
    let run = fixture.wait_state("Failed").await;
    let detail = JobDb::inspect_run(
      &fixture.job.spec.name,
      run.key,
      &fixture.state.inner.pool,
    )
    .await
    .unwrap();
    assert_eq!(
      detail.steps[0].exit_code,
      if mode == Mode::Nonzero { Some(7) } else { None }
    );
    assert_eq!(detail.steps[1].exit_code, None);
    assert!(detail.steps[1].container_id.is_none());
    assert_eq!(fixture.docker.runtimes.lock().unwrap().len(), 1);
    assert!(
      !serde_json::to_string(&detail)
        .unwrap()
        .contains("secret-value-must-not-persist")
    );
    fixture.cleanup().await;
  }
}

#[ntex::test]
async fn job_runtime_stop_cancels_next_step_and_wait_error_preserves_active_slot()
 {
  for mode in [Mode::BlockFirst, Mode::WaitError] {
    let fixture = Fixture::new(mode).await;
    crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
      .await
      .unwrap();
    if mode == Mode::BlockFirst {
      fixture.wait_blocked().await;
    } else {
      fixture.wait_state("Unknown").await;
      assert!(
        crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
          .await
          .is_err()
      );
    }
    crate::tasks::job::stop(&fixture.job.spec.name, &fixture.state)
      .await
      .unwrap();
    let run = fixture.wait_state("Interrupted").await;
    let steps = JobRunDb::steps(run.key, &fixture.state.inner.pool)
      .await
      .unwrap();
    assert_eq!(steps[0].exit_code, Some(137));
    assert!(steps[1].container_id.is_none());
    assert_eq!(fixture.docker.runtimes.lock().unwrap().len(), 1);
    fixture.cleanup().await;
  }
}

#[ntex::test]
async fn job_runtime_stop_publication_precedes_immediate_successor_start() {
  use nanocl_stubs::system::{Event, EventActorKind, EventKind};

  let fixture = Fixture::new(Mode::BlockFirst).await;
  let mut events = fixture.system.state.subscribe_raw(None).await.unwrap();
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  fixture.wait_blocked().await;
  let predecessor = fixture.current().await;
  tokio::time::timeout(Duration::from_secs(30), async {
    crate::tasks::job::stop(&fixture.job.spec.name, &fixture.state)
      .await
      .unwrap();
    // Admission may still be publishing the predecessor's terminal event.
    // An immediate follow-up must wait for that publication, then succeed.
    crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
      .await
      .unwrap();
    let successor = fixture.current().await;
    assert_ne!(predecessor.key, successor.key);
    let mut stopped = false;
    while let Some(bytes) = events.0.recv().await {
      if bytes.is_empty() {
        continue;
      }
      let event: Event = serde_json::from_slice(&bytes).unwrap();
      let Some(actor) = event.actor.as_ref() else {
        continue;
      };
      let Some(attributes) = actor.attributes.as_ref() else {
        continue;
      };
      if actor.kind != EventActorKind::Job
        || actor.key.as_deref() != Some(fixture.job.spec.name.as_str())
        || attributes["IncarnationKey"] != json!(fixture.job.incarnation_key)
        || attributes["SpecKey"] != json!(fixture.job.spec_key)
        || event.kind != EventKind::Normal
      {
        continue;
      }
      if event.action == NativeEventAction::Stop.to_string()
        && attributes["RunKey"] == json!(predecessor.key)
      {
        stopped = true;
      }
      if event.action == NativeEventAction::Starting.to_string()
        && attributes["RunKey"] == json!(successor.key)
      {
        assert!(stopped, "Successor Starting preceded predecessor Stop");
        return;
      }
    }
    panic!("Event stream ended before successor Starting");
  })
  .await
  .expect("Immediate successor admission or event publication timed out");
  // Stopping the predecessor releases the shared mock wait gate, so the
  // successor finishes naturally instead of requiring another cancellation.
  fixture.wait_state("Succeeded").await;
  fixture.cleanup().await;
}

#[ntex::test]
async fn job_runtime_delete_active_run_stops_it_and_retains_history() {
  let fixture = Fixture::new(Mode::BlockFirst).await;
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  fixture.wait_blocked().await;
  let run = fixture.current().await;
  crate::tasks::job::delete(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  let deadline = std::time::Instant::now() + Duration::from_secs(30);
  loop {
    let retained = JobRunDb::get(run.key, &fixture.state.inner.pool)
      .await
      .unwrap();
    let removed = JobDb::transform_read_by_pk(
      &fixture.job.spec.name,
      &fixture.state.inner.pool,
    )
    .await
    .is_err();
    if retained.finished_at.is_some() && removed {
      assert_eq!(retained.state, "Interrupted");
      break;
    }
    assert!(
      std::time::Instant::now() < deadline,
      "Active Job deletion did not settle: {retained:?}; removed={removed}"
    );
    ntex::time::sleep(Duration::from_millis(20)).await;
  }
  let detail = JobDb::inspect_run(
    &fixture.job.spec.name,
    run.key,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  assert_eq!(detail.run.state, JobRunState::Interrupted);
  assert_eq!(detail.steps[0].exit_code, Some(137));
  assert!(detail.steps[0].container_id.is_some());
  assert!(detail.steps[1].container_id.is_none());
  assert_eq!(detail.steps[1].exit_code, None);
  assert!(fixture.docker.runtimes.lock().unwrap().is_empty());
  fixture.cleanup().await;
}

#[ntex::test]
async fn job_runtime_recovery_never_dispatches_reserved_or_pending_work() {
  for creating in [false, true] {
    let fixture = Fixture::new(Mode::Success).await;
    let (accepted, owner) = fixture.accept(creating).await;
    if creating {
      JobRunDb::begin_step(
        &accepted.run,
        0,
        owner.clone(),
        &fixture.state.inner.pool,
      )
      .await
      .unwrap();
    }
    drop(owner);
    recovery::reconcile(&fixture.state).await.unwrap();
    let run = fixture.current().await;
    assert_eq!(run.state, if creating { "Unknown" } else { "Interrupted" });
    assert!(fixture.docker.runtimes.lock().unwrap().is_empty());
    assert!(
      !fixture
        .docker
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|request| request == "create" || request.ends_with("/start"))
    );
    if creating {
      crate::tasks::job::stop(&fixture.job.spec.name, &fixture.state)
        .await
        .unwrap();
      fixture.wait_state("Interrupted").await;
    }
    fixture.cleanup().await;
  }
}

#[ntex::test]
async fn job_runtime_old_ttl_cannot_delete_successor_run_or_recreated_job() {
  let mut fixture = Fixture::new(Mode::Success).await;
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  let first = fixture.wait_state("Succeeded").await;
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  let second = fixture.wait_state("Succeeded").await;
  assert_ne!(first.key, second.key);
  assert!(
    crate::tasks::job::delete_if_current(
      &fixture.job.spec.name,
      first.incarnation_key,
      first.key,
      &fixture.state
    )
    .await
    .is_err()
  );
  assert_eq!(fixture.current().await.key, second.key);
  crate::tasks::job::delete(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  for _ in 0..1500 {
    if JobDb::transform_read_by_pk(
      &fixture.job.spec.name,
      &fixture.state.inner.pool,
    )
    .await
    .is_err()
    {
      break;
    }
    ntex::time::sleep(Duration::from_millis(20)).await;
  }
  let partial: JobPartial = fixture.job.clone().into();
  fixture.job =
    JobDb::create_with_spec(&partial, "v0.19", &fixture.state.inner.pool)
      .await
      .unwrap();
  assert!(
    crate::tasks::job::delete_if_current(
      &fixture.job.spec.name,
      second.incarnation_key,
      second.key,
      &fixture.state
    )
    .await
    .is_err()
  );
  assert!(
    JobDb::transform_read_by_pk(
      &fixture.job.spec.name,
      &fixture.state.inner.pool
    )
    .await
    .is_ok()
  );
  assert_eq!(
    JobDb::inspect_run(
      &fixture.job.spec.name,
      first.key,
      &fixture.state.inner.pool
    )
    .await
    .unwrap()
    .run
    .key,
    first.key
  );
  fixture.cleanup().await;
}

/// Run explicitly against a disposable database/state directory. The Docker
/// daemon is shared, but every created container is a unique task-owned Job
/// step, network mode is none, and cleanup uses its recorded immutable ID.
#[ntex::test]
#[ignore = "requires NANOCL_JOB_REAL_DOCKER_TEST=1 and task-owned DB/state"]
async fn job_runtime_real_docker_two_runs_keep_independent_history() {
  use crate::objects::generic::ObjInspectByPk;
  use futures::FutureExt;
  assert_eq!(
    std::env::var("NANOCL_JOB_REAL_DOCKER_TEST").as_deref(),
    Ok("1")
  );
  assert!(std::env::var("NANOCL_TEST_STORE_ADDR").is_ok());
  assert!(std::env::var("NANOCL_TEST_STATE_DIR").is_ok());
  let mut fixture =
    Fixture::with_image(Mode::Success, "alpine:latest", true).await;
  let previous = &fixture.state;
  let docker = bollard_next::Docker::connect_with_unix(
    &previous.inner.config.docker_host,
    120,
    bollard_next::API_DEFAULT_VERSION,
  )
  .unwrap();
  fixture.state = SystemState {
    inner: Arc::new(SystemStateInner {
      pool: previous.inner.pool.clone(),
      docker_api: docker,
      config: previous.inner.config.clone(),
      task_manager: Default::default(),
      event_emitter: previous.inner.event_emitter.clone(),
      event_emitter_raw: Default::default(),
      arbiter: ntex::rt::Arbiter::current(),
    }),
  };
  let result = std::panic::AssertUnwindSafe(async {
    // Readiness uses the exact connection that execution will use and never
    // pulls an image. A slow/unavailable socket fails before accepting work.
    assert_eq!(fixture.state.inner.docker_api.ping().await.unwrap(), "OK");
    fixture
      .state
      .inner
      .docker_api
      .inspect_image("alpine:latest")
      .await
      .unwrap();
    assert_eq!(
      fixture.job.spec.image_pull_policy,
      Some(ImagePullPolicy::Never)
    );
    eprintln!("Job real Docker fixture {} ready", fixture.job.spec.name);
    let mut accepted = Vec::new();
    for _ in 0..2 {
      crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
        .await
        .unwrap();
      let deadline = std::time::Instant::now() + Duration::from_secs(180);
      let mut previous_phase = None;
      let run = loop {
        let run = fixture.current().await;
        let steps = JobRunDb::steps(run.key, &fixture.state.inner.pool)
          .await
          .unwrap();
        let phase = format!(
          "run={} state={} steps={:?}",
          run.key,
          run.state,
          steps
            .iter()
            .map(|step| (
              step.step_index,
              step.state.as_str(),
              step.container_id.is_some(),
              step.exit_code
            ))
            .collect::<Vec<_>>()
        );
        if previous_phase.as_ref() != Some(&phase) {
          let busy = lifecycle_lock(&fixture.job.spec.name)
            .try_lock_owned()
            .is_err();
          eprintln!("Job real Docker phase: {phase}; lifecycle_busy={busy}");
          previous_phase = Some(phase);
        }
        if run.finished_at.is_some() {
          assert_eq!(
            run.state, "Succeeded",
            "Job execution failed: {run:?}; steps={steps:?}"
          );
          let _settled =
            lifecycle_lock(&fixture.job.spec.name).lock_owned().await;
          break run;
        }
        assert_ne!(
          run.state, "Unknown",
          "Job execution uncertain: {run:?}; steps={steps:?}"
        );
        assert!(
          std::time::Instant::now() < deadline,
          "Job execution deadline: {run:?}; steps={steps:?}"
        );
        ntex::time::sleep(Duration::from_millis(100)).await;
      };
      let detail = JobDb::inspect_run(
        &fixture.job.spec.name,
        run.key,
        &fixture.state.inner.pool,
      )
      .await
      .unwrap();
      assert_eq!(detail.steps.len(), 2);
      assert!(detail.steps.iter().all(|step| step.exit_code == Some(0)
        && step.started_at.is_some()
        && step.finished_at.is_some()));
      assert_eq!(
        detail
          .steps
          .iter()
          .map(|step| step.step_index)
          .collect::<Vec<_>>(),
        vec![0, 1]
      );
      assert_eq!(detail.operation.outcome.as_deref(), Some("completed"));
      accepted.push(detail);
    }
    assert_ne!(accepted[0].run.key, accepted[1].run.key);
    assert_eq!(accepted[0].spec, accepted[1].spec);
    let ids = accepted
      .iter()
      .flat_map(|detail| {
        detail
          .steps
          .iter()
          .filter_map(|step| step.container_id.clone())
      })
      .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 4);
    let current =
      JobDb::inspect_obj_by_pk(&fixture.job.spec.name, &fixture.state)
        .await
        .unwrap();
    assert_eq!(current.current_run_key, Some(accepted[1].run.key));
    assert_eq!(current.instances.len(), 2);
    assert_eq!(current.instance_success, 2);
    assert_eq!(
      JobDb::runs(
        &fixture.job.spec.name,
        &Default::default(),
        &fixture.state.inner.pool
      )
      .await
      .unwrap()
      .len(),
      2
    );
  })
  .catch_unwind()
  .await;
  // A failed assertion must not leave a detached executor able to launch its
  // next step while container cleanup is running. Use the normal durable stop
  // path, then acquire its lifecycle lock before touching exact fixture IDs.
  let cleanup = ntex::time::timeout(Duration::from_secs(180), async {
    let live = JobDb::transform_read_by_pk(&fixture.job.spec.name, &fixture.state.inner.pool).await.unwrap();
    if let Some(key) = live.current_run_key {
      let run = JobRunDb::get(key, &fixture.state.inner.pool).await.unwrap();
      if run.finished_at.is_none() {
        loop {
          if crate::tasks::job::stop(&fixture.job.spec.name, &fixture.state).await.is_ok() {
            break;
          }
          ntex::time::sleep(Duration::from_millis(100)).await;
        }
        loop {
          if fixture.current().await.finished_at.is_some() {
            break;
          }
          ntex::time::sleep(Duration::from_millis(100)).await;
        }
      }
    }
    lifecycle_lock(&fixture.job.spec.name).lock_owned().await
  }).await.expect("Task-owned Job cleanup could not confirm cancellation; retained fixture state requires inspection");
  // Enumerate only this fixture's retained runs, then verify each immutable ID
  // before removing it. This also cleans already completed historical steps.
  let history = JobDb::runs(
    &fixture.job.spec.name,
    &Default::default(),
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  for retained in history {
    let run = JobRunDb::get(retained.key, &fixture.state.inner.pool)
      .await
      .unwrap();
    for (_, id, runtime) in inventory(&run, &fixture.state).await.unwrap() {
      assert!(runtime_step(&run, &runtime).is_ok());
      fixture
        .state
        .inner
        .docker_api
        .remove_container(
          &id,
          Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
          }),
        )
        .await
        .unwrap();
    }
  }
  drop(cleanup);
  fixture.cleanup().await;
  if let Err(panic) = result {
    std::panic::resume_unwind(panic);
  }
}

#[ntex::test]
async fn job_runtime_recovery_observes_running_step_without_launching_next() {
  let fixture = Fixture::new(Mode::Success).await;
  let (accepted, owner) = fixture.accept(true).await;
  JobRunDb::begin_step(
    &accepted.run,
    0,
    owner.clone(),
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  let config = runtime_config(
    &accepted.job,
    &accepted.run,
    0,
    &accepted.job.spec.containers[0],
    &fixture.state,
  )
  .await
  .unwrap();
  let id = format!("job-mock-recovery-{}", uuid::Uuid::new_v4().simple());
  fixture.docker.runtimes.lock().unwrap().push(Runtime {
    id: id.clone(),
    config,
    running: true,
    started: true,
    exit: None,
  });
  JobRunDb::attach_container(
    &accepted.run,
    0,
    &id,
    owner.clone(),
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  JobRunDb::update_step(
    &accepted.run,
    0,
    "Running",
    None,
    None,
    owner.clone(),
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  drop(owner);
  recovery::reconcile(&fixture.state).await.unwrap();
  assert_eq!(fixture.current().await.state, "Unknown");
  assert!(
    crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
      .await
      .is_err()
  );
  {
    let mut runtimes = fixture.docker.runtimes.lock().unwrap();
    runtimes[0].running = false;
    runtimes[0].exit = Some(0);
  }
  recovery::reconcile(&fixture.state).await.unwrap();
  let run = fixture.current().await;
  assert_eq!(run.state, "Interrupted");
  let steps = JobRunDb::steps(run.key, &fixture.state.inner.pool)
    .await
    .unwrap();
  assert_eq!(steps[0].exit_code, Some(0));
  assert!(steps[1].container_id.is_none());
  assert!(
    !fixture
      .docker
      .requests
      .lock()
      .unwrap()
      .iter()
      .any(|request| request == "create" || request.ends_with("/start"))
  );
  fixture.cleanup().await;
}

#[ntex::test]
async fn job_runtime_delayed_old_observation_cannot_rewrite_successor() {
  let fixture = Fixture::new(Mode::Success).await;
  let (first, first_owner) = fixture.accept(true).await;
  execute(&first, &first_owner, &fixture.state).await.unwrap();
  let (second, second_owner) = fixture.accept(true).await;
  execute(&second, &second_owner, &fixture.state)
    .await
    .unwrap();
  let before = JobDb::transform_read_by_pk(
    &fixture.job.spec.name,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  let retained = JobDb::inspect_run(
    &fixture.job.spec.name,
    first.run.key,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  let id = retained.steps[0].container_id.clone().unwrap();
  let mut delayed = fixture
    .docker
    .runtimes
    .lock()
    .unwrap()
    .iter()
    .find(|runtime| runtime.id == id)
    .unwrap()
    .clone();
  delayed.exit = Some(91);
  let runtime: ContainerInspectResponse =
    serde_json::from_value(delayed.inspect()).unwrap();
  record_observation(
    &first.run,
    0,
    &id,
    &runtime,
    &first_owner,
    &fixture.state,
  )
  .await
  .unwrap();
  finish(
    &first.run,
    "Failed",
    Some("Delayed previous executor result".into()),
    &first_owner,
    &fixture.state,
  )
  .await
  .unwrap();
  let after = JobDb::transform_read_by_pk(
    &fixture.job.spec.name,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  assert_eq!(after.current_run_key, Some(second.run.key));
  assert_eq!(after.status, before.status);
  assert_eq!(
    JobDb::inspect_run(
      &fixture.job.spec.name,
      first.run.key,
      &fixture.state.inner.pool
    )
    .await
    .unwrap(),
    retained
  );
  drop(first_owner);
  drop(second_owner);
  fixture.cleanup().await;
}

#[ntex::test]
async fn job_runtime_expected_delete_rejects_successor_and_pre_run_identity() {
  let fixture = Fixture::new(Mode::Success).await;
  let incarnation = fixture.job.incarnation_key;
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  let first = fixture.wait_state("Succeeded").await;
  assert!(
    crate::tasks::job::delete_if_identity(
      &fixture.job.spec.name,
      incarnation,
      None,
      &fixture.state
    )
    .await
    .is_err()
  );
  assert_eq!(fixture.current().await.key, first.key);
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  let second = fixture.wait_state("Succeeded").await;
  let before = JobDb::transform_read_by_pk(
    &fixture.job.spec.name,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  assert!(
    crate::tasks::job::delete_if_identity(
      &fixture.job.spec.name,
      incarnation,
      Some(first.key),
      &fixture.state
    )
    .await
    .is_err()
  );
  let after = JobDb::transform_read_by_pk(
    &fixture.job.spec.name,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  assert_eq!(after.current_run_key, Some(second.key));
  assert_eq!(after.status, before.status);
  fixture.cleanup().await;
}

#[ntex::test]
async fn job_runtime_revert_survives_caller_cancellation_without_starting() {
  let fixture = Fixture::new(Mode::BlockStop).await;
  // Compare retained revisions at the database timestamp precision.
  let expected_spec = JobDb::history_spec(
    &fixture.job.spec.name,
    fixture.job.spec_key,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  fixture.wait_blocked().await;
  let run = fixture.current().await;
  let mut revert = Box::pin(JobDb::revert_to_history(
    &fixture.job.spec.name,
    fixture.job.spec_key,
    "v0.19",
    &fixture.state,
  ));
  tokio::time::timeout(Duration::from_secs(30), async {
    let stopping = Box::pin(async {
      loop {
        if fixture
          .docker
          .requests
          .lock()
          .unwrap()
          .iter()
          .any(|path| path.ends_with("/stop"))
        {
          break;
        }
        ntex::time::sleep(Duration::from_millis(10)).await;
      }
    });
    if let futures::future::Either::Left((result, _)) =
      futures::future::select(&mut revert, stopping).await
    {
      panic!("Revert completed before deletion settled: {result:?}");
    }
  })
  .await
  .unwrap();
  drop(revert);
  fixture.docker.release.store(true, Ordering::SeqCst);
  fixture.docker.wake.notify_waiters();
  let restored = tokio::time::timeout(Duration::from_secs(30), async {
    loop {
      if let Ok(job) = JobDb::transform_read_by_pk(
        &fixture.job.spec.name,
        &fixture.state.inner.pool,
      )
      .await
        && job.incarnation_key != fixture.job.incarnation_key
      {
        break job;
      }
      ntex::time::sleep(Duration::from_millis(10)).await;
    }
  })
  .await
  .expect("Cancelled caller interrupted the accepted replacement");
  assert_ne!(restored.spec_key, fixture.job.spec_key);
  assert_eq!(
    JobPartial::from(restored.spec),
    JobPartial::from(expected_spec.clone())
  );
  assert!(restored.current_run_key.is_none());
  let detail = JobDb::inspect_run(
    &fixture.job.spec.name,
    run.key,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  assert_eq!(detail.run.state, JobRunState::Interrupted);
  assert!(detail.run.finished_at.is_some());
  assert_eq!(detail.spec, expected_spec);
  assert_eq!(detail.run.spec_key, fixture.job.spec_key);
  assert!(fixture.docker.runtimes.lock().unwrap().is_empty());
  assert_eq!(
    fixture
      .docker
      .requests
      .lock()
      .unwrap()
      .iter()
      .filter(|path| path.ends_with("/start"))
      .count(),
    1
  );
  fixture.cleanup().await;
}

#[ntex::test]
async fn job_runtime_revert_does_not_replace_an_unconfirmed_active_run() {
  let fixture = Fixture::new(Mode::StopUnconfirmed).await;
  crate::tasks::job::start(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  fixture.wait_blocked().await;
  let run = fixture.current().await;
  assert!(
    JobDb::revert_to_history(
      &fixture.job.spec.name,
      fixture.job.spec_key,
      "v0.19",
      &fixture.state
    )
    .await
    .is_err()
  );
  let retained = JobDb::transform_read_by_pk(
    &fixture.job.spec.name,
    &fixture.state.inner.pool,
  )
  .await
  .unwrap();
  assert_eq!(retained.incarnation_key, fixture.job.incarnation_key);
  assert_eq!(retained.spec_key, fixture.job.spec_key);
  assert_eq!(retained.current_run_key, Some(run.key));
  assert!(fixture.current().await.finished_at.is_none());
  {
    let mut runtimes = fixture.docker.runtimes.lock().unwrap();
    runtimes[0].running = false;
    runtimes[0].exit = Some(0);
  }
  crate::tasks::job::stop(&fixture.job.spec.name, &fixture.state)
    .await
    .unwrap();
  fixture.wait_state("Interrupted").await;
  fixture.docker.release.store(true, Ordering::SeqCst);
  fixture.docker.wake.notify_waiters();
  fixture.cleanup().await;
}
