use std::future::Future;

use futures::{StreamExt, TryStreamExt, stream::select_all};
use ntex::web;

use bollard_next::query_parameters::LogsOptions;
use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoResult,
};
use nanocl_stubs::process::{
  Process, ProcessKind, ProcessLogQuery, ProcessOutputLog,
};

use crate::{
  models::{JobDb, ProcessDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Jobs must use their current run's inventory without reading retained runs.
async fn select_log_inventory(
  kind: &ProcessKind,
  current_job: impl Future<Output = IoResult<Vec<Process>>>,
  by_name: impl Future<Output = IoResult<Vec<Process>>>,
) -> IoResult<Vec<Process>> {
  if *kind == ProcessKind::Job {
    current_job.await
  } else {
    by_name.await
  }
}

/// Get logs of a single process instance by it's name or id
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{name}/logs",
  params(
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("since" = Option<i64>, Query, description = "Only logs returned since timestamp"),
    ("until" = Option<i64>, Query, description = "Only logs returned until timestamp"),
    ("timestamps" = Option<bool>, Query, description = "Add timestamps to every log line"),
    ("follow" = Option<bool>, Query, description = "Boolean to return a stream or not"),
    ("tail" = Option<String>, Query, description = "Only return the n last (integer) or all ('all') logs"),
  ),
  responses(
    (status = 200, description = "Process instances logs", content_type = "application/vdn.nanocl.raw-stream"),
  ),
))]
pub async fn logs_process(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<ProcessLogQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  log::debug!("process::logs_process: {name}");
  let options = LogsOptions::from(qs.into_inner());
  let stream = state
    .inner
    .docker_api
    .logs(
      &name,
      Some(LogsOptions {
        stdout: true,
        stderr: true,
        ..options.clone()
      }),
    )
    .map(move |elem| match elem {
      Err(err) => Err(err),
      Ok(elem) => Ok(ProcessOutputLog {
        name: name.clone(),
        log: elem.into(),
      }),
    });
  let stream = utils::stream::transform_stream::<
    ProcessOutputLog,
    ProcessOutputLog,
  >(stream);
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(stream),
  )
}

/// Get logs of processes for a kind and canonical resource key
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{kind}/{key}/logs",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key in `{namespace}.{name}` format (jobs use their non-namespaced key)", example = "global.deploy-example"),
    ("since" = Option<i64>, Query, description = "Only logs returned since timestamp"),
    ("until" = Option<i64>, Query, description = "Only logs returned until timestamp"),
    ("timestamps" = Option<bool>, Query, description = "Add timestamps to every log line"),
    ("follow" = Option<bool>, Query, description = "Boolean to return a stream or not"),
    ("tail" = Option<String>, Query, description = "Only return the n last (integer) or all ('all') logs"),
  ),
  responses(
    (status = 200, description = "Process instances logs", content_type = "application/vdn.nanocl.raw-stream"),
  ),
))]
pub async fn logs_processes(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessLogQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  let processes = select_log_inventory(
    &kind,
    async {
      let job = JobDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      JobDb::current_processes(&job, &state.inner.pool).await
    },
    ProcessDb::read_by_kind_key(&key, None, &state.inner.pool),
  )
  .await?;
  let options = LogsOptions::from(qs.into_inner());
  let futures = processes
    .into_iter()
    .filter(|process| {
      if kind == ProcessKind::Cargo {
        !utils::container::cargo::is_retained_process_name(&process.name)
      } else {
        !process.name.starts_with("tmp-")
      }
    })
    .collect::<Vec<_>>()
    .into_iter()
    .map(|process| {
      state
        .inner
        .docker_api
        .logs(
          &process.data.id.unwrap_or_default(),
          Some(LogsOptions {
            stdout: true,
            stderr: true,
            ..options.clone()
          }),
        )
        .map(move |elem| match elem {
          Err(err) => Err(err),
          Ok(elem) => Ok(ProcessOutputLog {
            name: process.name.clone(),
            log: elem.into(),
          }),
        })
    })
    .collect::<Vec<_>>();
  let stream = select_all(futures).into_stream();
  let stream = utils::stream::transform_stream::<
    ProcessOutputLog,
    ProcessOutputLog,
  >(stream);
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(stream),
  )
}

#[cfg(test)]
mod tests {
  use futures::FutureExt;
  use nanocl_error::io::IoError;

  use super::*;

  fn process(id: &str, kind: ProcessKind) -> Process {
    Process {
      key: id.into(),
      created_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
      updated_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
      name: format!("backup-{id}"),
      kind,
      kind_key: "backup".into(),
      node_name: "test-node".into(),
      ip_address: None,
      data: bollard_next::models::ContainerInspectResponse {
        id: Some(id.into()),
        ..Default::default()
      },
    }
  }

  #[test]
  fn job_logs_select_current_run_inventory_and_never_fall_back_to_old_runs() {
    // A fresh same-name incarnation has no current run and must return no logs.
    for ids in [vec!["current-step-one", "current-step-two"], vec![]] {
      let expected = ids.clone();
      let inventory = select_log_inventory(
        &ProcessKind::Job,
        async {
          Ok(ids.iter().map(|id| process(id, ProcessKind::Job)).collect())
        },
        async {
          panic!(
            "Job logs must not query processes from earlier runs/incarnations"
          )
        },
      )
      .now_or_never()
      .unwrap()
      .unwrap();
      assert_eq!(
        inventory
          .iter()
          .map(|process| process.data.id.as_deref().unwrap())
          .collect::<Vec<_>>(),
        expected,
      );
    }
    let error = select_log_inventory(
      &ProcessKind::Job,
      async { Err(IoError::not_found("Job", "No live Job")) },
      async {
        panic!("Deleted Job must not fall back to retained process logs")
      },
    )
    .now_or_never()
    .unwrap()
    .unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::NotFound);
  }

  #[test]
  fn cargo_and_vm_logs_keep_the_existing_resource_inventory() {
    for kind in [ProcessKind::Cargo, ProcessKind::Vm] {
      let inventory = select_log_inventory(
        &kind,
        async { panic!("Cargo and VM logs must not read a Job") },
        async { Ok(vec![process("resource-container", kind.clone())]) },
      )
      .now_or_never()
      .unwrap()
      .unwrap();
      assert_eq!(inventory.len(), 1);
      assert_eq!(inventory[0].kind, kind);
      assert_eq!(inventory[0].key, "resource-container");
    }
  }
}
