use futures::{StreamExt, TryStreamExt, stream::select_all};
use ntex::web;

use bollard_next::query_parameters::LogsOptions;
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::{ProcessKind, ProcessLogQuery, ProcessOutputLog};

use crate::{
  models::{ProcessDb, SystemState},
  utils,
};

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
  let options =
    LogsOptions::try_from(qs.into_inner()).map_err(HttpError::bad_request)?;
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
  let processes =
    ProcessDb::read_by_kind_key(&key, None, &state.inner.pool).await?;
  let options =
    LogsOptions::try_from(qs.into_inner()).map_err(HttpError::bad_request)?;
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
