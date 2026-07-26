use futures::{StreamExt, TryStreamExt, stream::select_all};
use ntex::web;

use bollard_next::{
  container::WaitContainerOptions, secret::ContainerWaitExitError,
};
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::{ProcessWaitQuery, ProcessWaitResponse};

use crate::{
  models::{ProcessDb, SystemState},
  utils,
};

/// Wait for a all processes to reach a specific state
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{kind}/{key}/wait",
  params(
    ("kind" = String, Path, description = "Kind of the process", description = "Kind of the process instance eg: (cargo, job, vm)", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key (jobs use their non-namespaced key)"),
  ),
  responses(
    (status = 200, description = "Process wait stream", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Process does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::get("/processes/{kind}/{key}/wait")]
pub async fn wait_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessWaitQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  let opts = WaitContainerOptions {
    condition: qs.condition.clone().unwrap_or_default(),
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
#[web::get("/processes/{name}/wait")]
pub async fn wait_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<ProcessWaitQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  let opts = WaitContainerOptions {
    condition: qs.condition.clone().unwrap_or_default(),
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
