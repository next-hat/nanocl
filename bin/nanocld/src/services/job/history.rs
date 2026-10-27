use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::job::JobHistoryQuery;

use crate::models::{JobDb, SystemState};

/// List retained executions of every incarnation of a Job name.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{name}/histories",
  params(
    ("name" = String, Path, description = "Job name, including a removed job"),
    ("limit" = Option<i64>, Query, description = "Page size, 1 to 100; defaults to 50"),
    ("offset" = Option<i64>, Query, description = "Nonnegative offset; defaults to zero"),
  ),
  responses(
    (status = 200, description = "Runs, newest acceptance first", body = Vec<nanocl_stubs::job::JobRun>),
    (status = 400, description = "Invalid pagination", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn list_job_history(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  query: web::types::Query<JobHistoryQuery>,
) -> HttpResult<web::HttpResponse> {
  query.validate().map_err(HttpError::bad_request)?;
  let runs = JobDb::history(&path.1, &query, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&runs))
}

/// Inspect a retained execution without requiring the live Job to exist.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{name}/histories/{run}",
  params(
    ("name" = String, Path, description = "Job name, including a removed job"),
    ("run" = uuid::Uuid, Path, description = "Retained execution identity"),
  ),
  responses(
    (status = 200, description = "Pinned definition, initiating operation, and ordered step results; Docker logs are not archived", body = nanocl_stubs::job::JobRunInspect),
    (status = 404, description = "Run does not belong to the named Job", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn inspect_job_history(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let run = JobDb::inspect_run(&path.1, path.2, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&run))
}

#[cfg(test)]
mod tests {
  use bollard_next::models::ContainerCreateBody;
  use nanocl_stubs::job::{Job, JobPartial, JobRunState};
  use nanocld_client::{ConnectOpts, NanocldClient};

  use super::*;
  use crate::{
    models::{
      DistributedMutexDb, DistributedMutexPartial, JobRunDb, OperationDb,
      OperationOwner, Pool,
    },
    repositories::generic::RepositoryCreate,
    services,
    utils::tests::gen_default_test_system,
    vars,
  };

  async fn operation(job: &Job, action: &str, pool: &Pool) -> OperationDb {
    let mutex = DistributedMutexDb::create_from(
      DistributedMutexPartial {
        action: action.into(),
        resource_kind: "Job".into(),
        resource_key: job.spec.name.clone(),
        node_key: "job-history-fixture".into(),
        acquired_by: "job-history-fixture".into(),
      },
      pool,
    )
    .await
    .unwrap();
    let owner = OperationOwner {
      key: mutex.key,
      resource_kind: "Job".into(),
      resource_key: job.spec.name.clone(),
      incarnation_key: job.incarnation_key,
      node_name: mutex.node_key,
      owner_boot_id: uuid::Uuid::nil().to_string(),
      lock_device: 1,
      lock_inode: 1,
    };
    OperationDb::new(&owner, "Job", &job.spec.name, job.spec_key, action)
  }

  /// Real admission/persistence and HTTP/client serialization; runtime evidence
  /// is simulated through the same guarded step writes as the executor.
  #[ntex::test]
  async fn job_history_api_client_retains_runs_after_delete_and_recreation() {
    assert!(
      std::env::var_os("NANOCL_TEST_STORE_ADDR").is_some(),
      "history integration requires an explicitly selected disposable database"
    );
    assert!(
      std::env::var_os("NANOCL_TEST_STATE_DIR").is_some(),
      "history integration requires an isolated test state directory"
    );
    let system = gen_default_test_system().await;
    let state = system.state.clone();
    let server = web::test::server(async move |_| {
      web::App::<SystemState>::new()
        .with_config(services::app_config())
        .configure(services::ntex_config)
        .build_with(state.clone())
    });
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: server.url("").trim_end_matches('/').to_owned(),
      version: Some(format!("v{}", vars::VERSION)),
      ..Default::default()
    })
    .unwrap();
    let pool = &system.state.inner.pool;
    let name = format!("job-history-{}", uuid::Uuid::new_v4());
    let original = JobPartial {
      name: name.clone(),
      metadata: Some(serde_json::json!({"definition": "original"})),
      containers: vec![ContainerCreateBody::default(); 2],
      ..Default::default()
    };
    let mut job = JobDb::create_with_spec(&original, pool).await.unwrap();
    let mut runs = Vec::new();
    for _ in 0..2 {
      let accepted =
        JobDb::accept_run(&job, operation(&job, "start", pool).await, (), pool)
          .await
          .unwrap();
      assert!(
        JobDb::complete_start(&accepted.operation, (), pool)
          .await
          .unwrap()
      );
      let detail = client
        .inspect_history_job(&name, &accepted.run.key.to_string())
        .await
        .unwrap();
      assert!(detail.operation.completed_at.is_some());
      assert_eq!(detail.run.state, JobRunState::Pending);
      for index in 0..2 {
        assert!(
          JobRunDb::begin_step(&accepted.run, index, (), pool)
            .await
            .unwrap()
        );
        let id = format!("{}-{index}", accepted.run.key);
        assert!(
          JobRunDb::attach_container(&accepted.run, index, &id, (), pool)
            .await
            .unwrap()
        );
        assert!(
          JobRunDb::update_step(
            &accepted.run,
            index,
            "Running",
            None,
            None,
            (),
            pool
          )
          .await
          .unwrap()
        );
        assert!(
          JobRunDb::update_step(
            &accepted.run,
            index,
            "Succeeded",
            Some(0),
            None,
            (),
            pool
          )
          .await
          .unwrap()
        );
      }
      assert!(
        JobRunDb::finish(&accepted.run, "Succeeded", None, (), pool)
          .await
          .unwrap()
      );
      runs.push(accepted.run.key);
      job = accepted.job;
    }
    assert_ne!(runs[0], runs[1]);
    let delete = operation(&job, "delete", pool).await;
    JobDb::accept_control(&job, delete.clone(), job.current_run_key, (), pool)
      .await
      .unwrap();
    assert!(
      JobDb::delete_operation(&delete, job.current_run_key, (), pool)
        .await
        .unwrap()
    );
    let first_page = client
      .list_history_job(
        &name,
        Some(&JobHistoryQuery {
          limit: Some(1),
          offset: Some(0),
        }),
      )
      .await
      .unwrap();
    let second_page = client
      .list_history_job(
        &name,
        Some(&JobHistoryQuery {
          limit: Some(1),
          offset: Some(1),
        }),
      )
      .await
      .unwrap();
    assert_eq!(first_page.len(), 1);
    assert_eq!(second_page.len(), 1);
    assert_eq!(first_page[0].key, runs[1]);
    assert_eq!(second_page[0].key, runs[0]);
    let recreated = JobDb::create_with_spec(
      &JobPartial {
        name: name.clone(),
        metadata: Some(serde_json::json!({"definition": "replacement"})),
        ..Default::default()
      },
      pool,
    )
    .await
    .unwrap();
    assert_ne!(recreated.incarnation_key, job.incarnation_key);
    for key in runs {
      let detail = client
        .inspect_history_job(&name, &key.to_string())
        .await
        .unwrap();
      assert_eq!(detail.run.state, JobRunState::Succeeded);
      assert_eq!(detail.run.incarnation_key, job.incarnation_key);
      assert_eq!(detail.run.spec_key, job.spec_key);
      assert_eq!(detail.spec, original.clone().into());
      assert_eq!(
        detail
          .steps
          .iter()
          .map(|step| step.step_index)
          .collect::<Vec<_>>(),
        [0, 1]
      );
      assert!(
        detail
          .steps
          .iter()
          .all(|step| step.exit_code == Some(0) && step.container_id.is_some())
      );
    }
    for (path, status) in [
      (
        format!("/jobs/{name}/histories?limit=0"),
        ntex::http::StatusCode::BAD_REQUEST,
      ),
      (
        format!("/jobs/{name}/histories?limit=101"),
        ntex::http::StatusCode::BAD_REQUEST,
      ),
      (
        format!("/jobs/{name}/histories?offset=-1"),
        ntex::http::StatusCode::BAD_REQUEST,
      ),
      (
        format!("/jobs/other-name/histories/{}", first_page[0].key),
        ntex::http::StatusCode::NOT_FOUND,
      ),
      (
        format!("/jobs/{name}/histories/{}", uuid::Uuid::nil()),
        ntex::http::StatusCode::NOT_FOUND,
      ),
    ] {
      let response = system.client.send_get(&path, None::<String>).await;
      assert_eq!(response.status(), status, "{path}");
    }
    let delete = operation(&recreated, "delete", pool).await;
    JobDb::accept_control(&recreated, delete.clone(), None, (), pool)
      .await
      .unwrap();
    assert!(
      JobDb::delete_operation(&delete, None, (), pool)
        .await
        .unwrap()
    );
  }
}
