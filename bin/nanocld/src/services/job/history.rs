use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::job::JobHistoryQuery;

use crate::models::{JobDb, SystemState};

/// List saved configurations of every retained incarnation of a Job name.
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
    (status = 200, description = "Configurations, newest creation first", body = Vec<nanocl_stubs::job::JobSpec>),
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
  let specs = JobDb::history(&path.1, &query, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&specs))
}

/// Inspect a saved configuration without requiring the live Job to exist.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{name}/histories/{id}",
  params(
    ("name" = String, Path, description = "Job name, including a removed job"),
    ("id" = uuid::Uuid, Path, description = "Saved configuration identity"),
  ),
  responses(
    (status = 200, description = "Saved Job configuration", body = nanocl_stubs::job::JobSpec),
    (status = 404, description = "Configuration does not belong to the named Job", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn inspect_job_history(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let spec = JobDb::history_spec(&path.1, path.2, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&spec))
}

/// Restore a retained configuration without executing the replacement Job.
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Jobs",
  path = "/jobs/{name}/histories/{id}/revert",
  params(
    ("name" = String, Path, description = "Name of the live Job to replace"),
    ("id" = uuid::Uuid, Path, description = "Saved configuration identity belonging to this Job"),
  ),
  responses(
    (status = 200, description = "Job configuration restored without starting a run", body = nanocl_stubs::job::Job),
    (status = 400, description = "Saved configuration is invalid", body = crate::services::openapi::ApiError),
    (status = 404, description = "Job or its saved configuration does not exist", body = crate::services::openapi::ApiError),
    (status = 409, description = "Job identity changed or another lifecycle action is pending", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn revert_job(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let job = JobDb::revert_to_history(&path.1, path.2, &path.0, state).await?;
  Ok(web::HttpResponse::Ok().json(&job))
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
    repositories::generic::{RepositoryCreate, RepositoryReadByTransform},
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
  async fn job_history_and_runs_retain_configurations_after_recreation() {
    // Use the shared integration fixture, including its prepared CI defaults.
    // NANOCL_TEST_STORE_ADDR and NANOCL_TEST_STATE_DIR remain optional overrides.
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
    JobDb::create_with_spec(&original, "v0.19", pool)
      .await
      .unwrap();
    let mut job = JobDb::transform_read_by_pk(&name, pool).await.unwrap();
    // A configuration is visible before its first execution.
    let revisions = client.list_history_job(&name, None).await.unwrap();
    assert_eq!(revisions, vec![job.spec.clone()]);
    assert!(client.list_runs_job(&name, None).await.unwrap().is_empty());
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
        .inspect_run_job(&name, &accepted.run.key.to_string())
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
    assert_eq!(
      client.list_history_job(&name, None).await.unwrap(),
      vec![job.spec.clone()],
    );
    let first_page = client
      .list_runs_job(
        &name,
        Some(&JobHistoryQuery {
          limit: Some(1),
          offset: Some(0),
        }),
      )
      .await
      .unwrap();
    let second_page = client
      .list_runs_job(
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
    JobDb::create_with_spec(
      &JobPartial {
        name: name.clone(),
        metadata: Some(serde_json::json!({"definition": "replacement"})),
        ..Default::default()
      },
      "v0.19",
      pool,
    )
    .await
    .unwrap();
    let recreated = JobDb::transform_read_by_pk(&name, pool).await.unwrap();
    assert_ne!(recreated.incarnation_key, job.incarnation_key);
    // Statefile replacement must retain the old configuration independently
    // of run count, and expose the replacement even before it runs.
    let revisions = client.list_history_job(&name, None).await.unwrap();
    assert_eq!(revisions, vec![recreated.spec.clone(), job.spec.clone()]);
    let page = client
      .list_history_job(
        &name,
        Some(&JobHistoryQuery {
          limit: Some(1),
          offset: Some(1),
        }),
      )
      .await
      .unwrap();
    assert_eq!(page, vec![job.spec.clone()]);
    for expected in [&job.spec, &recreated.spec] {
      let revision = client
        .inspect_history_job(&name, &expected.key.to_string())
        .await
        .unwrap();
      assert_eq!(&revision, expected);
    }
    for key in runs {
      let detail = client
        .inspect_run_job(&name, &key.to_string())
        .await
        .unwrap();
      assert_eq!(detail.run.state, JobRunState::Succeeded);
      assert_eq!(detail.run.incarnation_key, job.incarnation_key);
      assert_eq!(detail.run.spec_key, job.spec_key);
      assert_eq!(detail.spec.key, job.spec_key);
      assert_eq!(detail.spec.job_key, name);
      assert_eq!(detail.spec.version, "v0.19");
      assert_eq!(detail.spec.created_at, job.spec.created_at);
      assert_eq!(detail.spec, job.spec);
      assert_eq!(JobPartial::from(detail.spec.clone()), original);
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
        format!("/jobs/other-name/histories/{}", job.spec_key),
        ntex::http::StatusCode::NOT_FOUND,
      ),
      (
        format!("/jobs/{name}/histories/{}", first_page[0].key),
        ntex::http::StatusCode::NOT_FOUND,
      ),
      (
        format!("/jobs/{name}/runs?limit=0"),
        ntex::http::StatusCode::BAD_REQUEST,
      ),
      (
        format!("/jobs/{name}/runs?limit=101"),
        ntex::http::StatusCode::BAD_REQUEST,
      ),
      (
        format!("/jobs/{name}/runs?offset=-1"),
        ntex::http::StatusCode::BAD_REQUEST,
      ),
      (
        format!("/jobs/other-name/runs/{}", first_page[0].key),
        ntex::http::StatusCode::NOT_FOUND,
      ),
      (
        format!("/jobs/{name}/runs/{}", uuid::Uuid::nil()),
        ntex::http::StatusCode::NOT_FOUND,
      ),
    ] {
      let response = system.client.send_get(&path, None::<String>).await;
      assert_eq!(response.status(), status, "{path}");
    }
    // Reverting must scope the selected revision before changing the live Job.
    let foreign = JobDb::create_with_spec(
      &JobPartial {
        name: format!("{name}-foreign"),
        ..Default::default()
      },
      "v0.19",
      pool,
    )
    .await
    .unwrap();
    for key in [foreign.spec_key, first_page[0].key, uuid::Uuid::nil()] {
      let response = system
        .client
        .send_patch(
          &format!("/jobs/{name}/histories/{key}/revert"),
          None::<String>,
          None::<String>,
        )
        .await;
      assert_eq!(response.status(), ntex::http::StatusCode::NOT_FOUND);
      assert_eq!(
        JobDb::transform_read_by_pk(&name, pool).await.unwrap(),
        recreated
      );
    }
    crate::tasks::job::delete_for_replacement(&foreign, &system.state)
      .await
      .unwrap();
    let restored = client
      .revert_job(&name, &job.spec_key.to_string())
      .await
      .unwrap();
    assert_ne!(restored.incarnation_key, recreated.incarnation_key);
    assert_ne!(restored.spec_key, job.spec_key);
    assert_ne!(restored.spec_key, recreated.spec_key);
    assert_eq!(JobPartial::from(restored.spec.clone()), original);
    assert!(restored.current_run_key.is_none());
    assert_eq!(client.list_runs_job(&name, None).await.unwrap().len(), 2);
    assert_eq!(
      client
        .inspect_history_job(&name, &job.spec_key.to_string())
        .await
        .unwrap(),
      job.spec,
    );
    // The current revision is also a valid target and still does not run.
    let restored_again = client
      .revert_job(&name, &restored.spec_key.to_string())
      .await
      .unwrap();
    assert_ne!(restored_again.incarnation_key, restored.incarnation_key);
    assert_eq!(JobPartial::from(restored_again.spec.clone()), original);
    assert!(restored_again.current_run_key.is_none());
    let revisions = client.list_history_job(&name, None).await.unwrap();
    assert_eq!(revisions.len(), 4);
    assert_eq!(&revisions[2..], &[recreated.spec.clone(), job.spec.clone()]);
    let delete = operation(&restored_again, "delete", pool).await;
    JobDb::accept_control(&restored_again, delete.clone(), None, (), pool)
      .await
      .unwrap();
    assert!(
      JobDb::delete_operation(&delete, None, (), pool)
        .await
        .unwrap()
    );
    assert_eq!(
      client.list_history_job(&name, None).await.unwrap(),
      revisions
    );
    assert_eq!(client.list_runs_job(&name, None).await.unwrap().len(), 2);
    let response = system
      .client
      .send_patch(
        &format!("/jobs/{name}/histories/{}/revert", job.spec_key),
        None::<String>,
        None::<String>,
      )
      .await;
    assert_eq!(response.status(), ntex::http::StatusCode::NOT_FOUND);
  }
}
