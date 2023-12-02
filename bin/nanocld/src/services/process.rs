use ntex::web;
use futures_util::stream::select_all;
use futures_util::{StreamExt, TryStreamExt};

use nanocl_error::http::HttpResult;

use bollard_next::container::LogsOptions;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::process::{ProcessLogQuery, ProcessOutputLog};

use crate::utils;
use crate::models::{DaemonState, ProcessDb, ProcessKind};

/// Get logs of an process
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{kind}/{name}/logs",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace of the process"),
    ("since" = Option<i64>, Query, description = "Only logs returned since timestamp"),
    ("until" = Option<i64>, Query, description = "Only logs returned until timestamp"),
    ("timestamps" = Option<bool>, Query, description = "Add timestamps to every log line"),
    ("follow" = Option<bool>, Query, description = "Boolean to return a stream or not"),
    ("tail" = Option<String>, Query, description = "Only return the n last (integer) or all (\"all\") logs"),
  ),
  responses(
    (status = 200, description = "Process instances logs", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Process don't exist"),
  ),
))]
#[web::get("/processes/{kind}/{name}/logs")]
async fn logs_process(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessLogQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind: ProcessKind = kind.try_into()?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  let processes = ProcessDb::find_by_kind_key(&kind_key, &state.pool).await?;
  log::debug!("process::logs_process: {processes:#?}");
  let options: LogsOptions<String> = qs.into_inner().into();
  let futures = processes
    .into_iter()
    .map(|process| {
      state
        .docker_api
        .logs(
          &process.data.id.unwrap_or_default(),
          Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            ..options.clone()
          }),
        )
        .map(move |elem| match elem {
          Err(err) => Err(err),
          Ok(elem) => {
            log::debug!("{:#?} {elem}", &process.name);
            Ok(ProcessOutputLog {
              name: process.name.clone(),
              log: elem.into(),
            })
          }
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

/// Start a process
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{name}/start",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace where the process belongs is needed"),
  ),
  responses(
    (status = 202, description = "Process started"),
    (status = 404, description = "Process does not exist"),
  ),
))]
#[web::post("/processes/{type}/{name}/start")]
pub(crate) async fn start_process(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind: ProcessKind = kind.try_into()?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  utils::process::start_by_kind(&kind, &kind_key, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Stop a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{name}/stop",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace where the process belongs is needed"),
  ),
  responses(
    (status = 202, description = "Cargo stopped"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/processes/{kind}/{name}/stop")]
pub(crate) async fn stop_process(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind: ProcessKind = kind.try_into()?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  utils::process::stop_by_kind(&kind, &kind_key, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(logs_process);
  config.service(start_process);
  config.service(stop_process);
}
