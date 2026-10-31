use futures::{StreamExt, TryStreamExt, stream::select_all};
use ntex::web;

use bollard_next::{
  models::ContainerWaitExitError, query_parameters::WaitContainerOptions,
};
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{
  job::JobRunInspect,
  process::{
    JobWaitFrame, ProcessKind, ProcessWaitQuery, ProcessWaitResponse,
    WaitCondition,
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

fn job_wait_frames(run: JobRunInspect) -> Vec<JobWaitFrame> {
  let mut frames = run
    .steps
    .into_iter()
    .filter_map(|step| {
      step.exit_code.map(|status_code| JobWaitFrame::Step {
        response: ProcessWaitResponse {
          process_name: step
            .container_id
            .unwrap_or_else(|| format!("step-{}", step.step_index)),
          status_code,
          error: step.error.map(|message| ContainerWaitExitError {
            message: Some(message),
          }),
        },
      })
    })
    .collect::<Vec<_>>();
  frames.push(JobWaitFrame::Terminal {
    run_key: run.run.key,
    state: run.run.state,
    reason: run.run.reason,
  });
  frames
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
          return Ok::<_, HttpError>(run);
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
  });
  let stream = stream
    .map(|result| {
      let frames = match result {
        Ok(run) => job_wait_frames(run),
        Err(err) => vec![JobWaitFrame::Error {
          message: match err.msg.as_str() {
            "Job incarnation changed while waiting" => {
              "Job incarnation changed while waiting"
            }
            "Job was replaced before a run was accepted" => {
              "Job was replaced before a run was accepted"
            }
            _ => "Unable to inspect the pinned Job run",
          }
          .into(),
        }],
      };
      futures::stream::iter(frames.into_iter().map(Ok::<_, HttpError>))
    })
    .flatten()
    .boxed_local();
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(
        utils::stream::transform_stream::<JobWaitFrame, JobWaitFrame>(stream),
      ),
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_stubs::job::{
    JobRun, JobRunOperation, JobRunState, JobRunStep, JobSpec, JobStepState,
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
      job_wait_frames(history(JobRunState::Failed, &[Some(17), None, Some(0)]));
    assert_eq!(results.len(), 3);
    assert!(
      matches!(&results[0], JobWaitFrame::Step {response} if response.process_name == "immutable-0" && response.status_code == 17)
    );
    assert!(
      matches!(&results[1], JobWaitFrame::Step {response} if response.process_name == "immutable-2" && response.status_code == 0)
    );
    assert!(matches!(
      &results[2],
      JobWaitFrame::Terminal {
        state: JobRunState::Failed,
        ..
      }
    ));
  }

  #[test]
  fn job_wait_reports_pinned_terminal_without_fabricated_exit() {
    for state in [JobRunState::Failed, JobRunState::Interrupted] {
      let run = history(state, &[None]);
      let run_key = run.run.key;
      let frames = job_wait_frames(run);
      assert_eq!(frames.len(), 1);
      assert!(matches!(&frames[0], JobWaitFrame::Terminal {
        run_key: actual, state: actual_state, reason: Some(reason),
      } if *actual == run_key && *actual_state == state && reason == "recorded outcome"));
    }
    let frames = job_wait_frames(history(JobRunState::Succeeded, &[Some(0)]));
    assert!(
      matches!(&frames[0], JobWaitFrame::Step { response } if response.status_code == 0)
    );
    assert!(matches!(
      &frames[1],
      JobWaitFrame::Terminal {
        state: JobRunState::Succeeded,
        ..
      }
    ));
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
