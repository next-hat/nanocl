use futures::{stream::select_all, StreamExt, TryStreamExt};
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
  path = "/processes/{kind}/{name}/wait",
  params(
    ("name" = String, Path, description = "Name of the job instance usually `name` or `name-number`"),
  ),
  responses(
    (status = 200, description = "Job wait", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Job does not exist"),
  ),
))]
#[web::get("/processes/{kind}/{name}/wait")]
pub async fn wait_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessWaitQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  let kind_pk = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  let opts = WaitContainerOptions {
    condition: qs.condition.clone().unwrap_or_default(),
  };
  let processes =
    ProcessDb::read_by_kind_key(&kind_pk, None, &state.inner.pool).await?;
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
