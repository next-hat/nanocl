use futures::{StreamExt, TryStreamExt, stream::select_all};
use ntex::web;

use bollard_next::{
  models::ContainerWaitExitError, query_parameters::WaitContainerOptions,
};
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{
  job::{JobRunInspect, JobRunState},
  process::{
    ProcessKind, ProcessWaitQuery, ProcessWaitResponse, WaitCondition,
  },
};

use crate::{
  models::{JobDb, JobRunDb, ProcessDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Wait for a all processes to reach a specific state
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{kind}/{key}/wait",
  params(
    ("kind" = String, Path, description = "Kind of the process", description = "Kind of the process instance eg: (cargo, job, vm)", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key in `{namespace}.{name}` format (jobs use their non-namespaced key)"),
  ),
  responses(
    (status = 200, description = "Process wait stream", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Process does not exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn wait_processes(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessWaitQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  if kind == ProcessKind::Job {
    if matches!(
      qs.condition,
      Some(WaitCondition::Removed | WaitCondition::NextExit)
    ) {
      return Err(HttpError::bad_request(
        "Job wait pins the current run; use the default or not-running condition",
      ));
    }
    return wait_job(&key, state).await;
  }
  let opts = WaitContainerOptions {
    condition: qs.condition.clone().unwrap_or_default().into(),
  };
  let processes =
    ProcessDb::read_by_kind_key(&key, None, &state.inner.pool).await?;
  let mut streams = Vec::new();
  for process in processes {
    let options = Some(opts.clone());
    let stream = state
      .inner
      .docker_api
      .wait_container(&process.key, options)
      .map(move |wait_result| match wait_result {
        Err(err) => {
          if let bollard_next::errors::Error::DockerContainerWaitError {
            error,
            code,
          } = &err
          {
            return Ok(ProcessWaitResponse {
              process_name: process.name.clone(),
              status_code: *code,
              error: Some(ContainerWaitExitError {
                message: Some(error.to_owned()),
              }),
            });
          }
          Err(err)
        }
        Ok(wait_response) => {
          Ok(ProcessWaitResponse::from_container_wait_response(
            wait_response,
            process.name.clone(),
          ))
        }
      });
    streams.push(stream);
  }
  let stream = select_all(streams).into_stream();
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(utils::stream::transform_stream::<
        ProcessWaitResponse,
        ProcessWaitResponse,
      >(stream)),
  )
}

/// Only observed exit codes are returned. A run interrupted before creation
/// fails the stream rather than inventing a container exit code.
fn job_wait_outcome(
  run: JobRunInspect,
) -> Vec<HttpResult<ProcessWaitResponse>> {
  let mut results = run
    .steps
    .into_iter()
    .filter_map(|step| {
      step.exit_code.map(|status_code| {
        Ok(ProcessWaitResponse {
          process_name: step
            .container_id
            .unwrap_or_else(|| format!("step-{}", step.step_index)),
          status_code,
          error: step.error.map(|message| ContainerWaitExitError {
            message: Some(message),
          }),
        })
      })
    })
    .collect::<Vec<_>>();
  if run.run.state != JobRunState::Succeeded {
    results.push(Err(HttpError::conflict(run.run.reason.unwrap_or_else(
      || format!("Job run {} ended as {:?}", run.run.key, run.run.state),
    ))));
  }
  results
}

async fn wait_job(
  key: &str,
  state: &SystemState,
) -> HttpResult<web::HttpResponse> {
  let job = JobDb::transform_read_by_pk(key, &state.inner.pool).await?;
  let key = key.to_owned();
  let state = state.clone();
  let stream = futures::stream::once(async move {
    let mut run_key = job.current_run_key;
    loop {
      if let Some(run_key) = run_key {
        let run = JobRunDb::get(run_key, &state.inner.pool).await?;
        if run.job_name != key || run.incarnation_key != job.incarnation_key {
          return Err(HttpError::conflict(
            "Job incarnation changed while waiting",
          ));
        }
        if run.finished_at.is_some() {
          let run =
            JobDb::inspect_run(&key, run_key, &state.inner.pool).await?;
          return Ok::<_, HttpError>(job_wait_outcome(run));
        }
      } else {
        // A subscription before the first start pins the incarnation and then
        // its first admitted run; later runs never redirect this waiter.
        let current =
          JobDb::transform_read_by_pk(&key, &state.inner.pool).await?;
        if current.incarnation_key != job.incarnation_key {
          return Err(HttpError::conflict(
            "Job was replaced before a run was accepted",
          ));
        }
        run_key = current.current_run_key;
      }
      ntex::time::sleep(std::time::Duration::from_millis(200)).await;
    }
  })
  .map_ok(futures::stream::iter)
  .try_flatten()
  .boxed_local();
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(utils::stream::transform_stream::<
        ProcessWaitResponse,
        ProcessWaitResponse,
      >(stream)),
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_stubs::job::{
    JobRun, JobRunOperation, JobRunStep, JobSpec, JobStepState,
  };

  fn history(state: JobRunState, codes: &[Option<i64>]) -> JobRunInspect {
    let key = uuid::Uuid::new_v4();
    let incarnation_key = uuid::Uuid::new_v4();
    let spec_key = uuid::Uuid::new_v4();
    let operation_key = uuid::Uuid::new_v4();
    let now = chrono::Utc::now().naive_utc();
    JobRunInspect {
      run: JobRun {
        key,
        job_name: "wait-fixture".into(),
        incarnation_key,
        spec_key,
        operation_key,
        accepted_at: now,
        started_at: Some(now),
        finished_at: Some(now),
        state,
        reason: Some("recorded outcome".into()),
      },
      spec: JobSpec::default(),
      operation: JobRunOperation {
        key: operation_key,
        incarnation_key,
        resource_kind: "Job".into(),
        resource_key: "wait-fixture".into(),
        action: "start".into(),
        spec_key,
        accepted_at: now,
        completed_at: Some(now),
        outcome: Some("completed".into()),
        diagnostic: None,
      },
      steps: codes
        .iter()
        .enumerate()
        .map(|(index, code)| JobRunStep {
          run_key: key,
          step_index: index as i32,
          container_id: Some(format!("immutable-{index}")),
          state: if code == &Some(0) {
            JobStepState::Succeeded
          } else {
            JobStepState::Interrupted
          },
          started_at: None,
          finished_at: Some(now),
          exit_code: *code,
          error: None,
        })
        .collect(),
    }
  }

  #[test]
  fn job_wait_preserves_step_order_and_actual_exit_codes() {
    let results =
      job_wait_outcome(history(JobRunState::Succeeded, &[Some(0), Some(0)]));
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].as_ref().unwrap().process_name, "immutable-0");
    assert_eq!(results[1].as_ref().unwrap().process_name, "immutable-1");
    assert!(
      results
        .iter()
        .all(|result| result.as_ref().unwrap().status_code == 0)
    );
    let failed =
      job_wait_outcome(history(JobRunState::Failed, &[Some(17), None]));
    assert_eq!(failed.len(), 2);
    assert_eq!(failed[0].as_ref().unwrap().status_code, 17);
    assert!(failed[1].is_err());
  }

  #[test]
  fn job_wait_does_not_manufacture_exit_codes_for_unobserved_steps() {
    let interrupted =
      job_wait_outcome(history(JobRunState::Interrupted, &[None, None]));
    assert_eq!(interrupted.len(), 1);
    assert!(interrupted[0].is_err());
  }
}

/// Wait for a single process to reach a specific state
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{name}/wait",
  params(
    ("name" = String, Path, description = "Name or id of the process instance"),
    ("condition" = Option<String>, Query, description = "Wait condition: not-running, next-exit, removed"),
  ),
  responses(
    (status = 200, description = "Process wait stream", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Process does not exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn wait_process(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<ProcessWaitQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  let opts = WaitContainerOptions {
    condition: qs.condition.clone().unwrap_or_default().into(),
  };
  let stream = state
    .inner
    .docker_api
    .wait_container(&name, Some(opts))
    .map(move |wait_result| match wait_result {
      Err(err) => {
        if let bollard_next::errors::Error::DockerContainerWaitError {
          error,
          code,
        } = &err
        {
          return Ok(ProcessWaitResponse {
            process_name: name.clone(),
            status_code: *code,
            error: Some(ContainerWaitExitError {
              message: Some(error.to_owned()),
            }),
          });
        }
        Err(err)
      }
      Ok(wait_response) => {
        Ok(ProcessWaitResponse::from_container_wait_response(
          wait_response,
          name.clone(),
        ))
      }
    });
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(utils::stream::transform_stream::<
        ProcessWaitResponse,
        ProcessWaitResponse,
      >(stream)),
  )
}
