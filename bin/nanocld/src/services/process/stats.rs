use futures::{StreamExt, TryStreamExt, stream::select_all};
use ntex::web;

use bollard_next::container::StatsOptions;
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::{ProcessStats, ProcessStatsQuery};

use crate::{
  models::{ProcessDb, SystemState},
  utils,
};

/// Get stats of all processes for a kind and canonical resource key
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{kind}/{key}/stats",
  params(
    ("kind" = String, Path, description = "Kind of process", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key in `{namespace}.{name}` format (jobs use their non-namespaced key)", example = "global.deploy-example"),
    ("stream" = Option<bool>, Query, description = "Return a stream of stats"),
    ("one_shot" = Option<bool>, Query, description = "Return stats only once"),
  ),
  responses(
    (status = 200, description = "Process stats", content_type = "application/vdn.nanocl.raw-stream", body = ProcessStats),
    (status = 404, description = "Process does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::get("/processes/{kind}/{key}/stats")]
pub async fn stats_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessStatsQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  let opts: StatsOptions = qs.clone().into();
  let processes =
    ProcessDb::read_by_kind_key(&key, None, &state.inner.pool).await?;
  let streams =
    processes
      .into_iter()
      .map(|process| {
        state.inner.docker_api.stats(&process.key, Some(opts)).map(
          move |elem| match elem {
            Err(err) => Err(err),
            Ok(stats) => Ok(ProcessStats {
              name: process.name.clone(),
              stats,
            }),
          },
        )
      })
      .collect::<Vec<_>>();
  let stream = select_all(streams).into_stream();
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(
        utils::stream::transform_stream::<ProcessStats, ProcessStats>(stream),
      ),
  )
}

/// Get stats of a process by it's key name
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/process/{name}/stats",
  params(
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("stream" = Option<bool>, Query, description = "Return a stream of stats"),
    ("one_shot" = Option<bool>, Query, description = "Return stats only once"),
  ),
  responses(
    (status = 200, description = "Process stats", content_type = "application/vdn.nanocl.raw-stream", body = ProcessStats),
    (status = 404, description = "Process does not exist", body = crate::services::openapi::ApiError),
  )
))]
#[web::get("/process/{name}/stats")]
pub async fn process_stats_by_name(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<ProcessStatsQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, process) = path.into_inner();
  let opts: StatsOptions = qs.clone().into();
  let stream =
    state
      .inner
      .docker_api
      .stats(&process, Some(opts))
      .map(move |elem| match elem {
        Ok(stats) => Ok(ProcessStats {
          name: process.clone(),
          stats,
        }),
        Err(err) => Err(err),
      });
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(
        utils::stream::transform_stream::<ProcessStats, ProcessStats>(stream),
      ),
  )
}
