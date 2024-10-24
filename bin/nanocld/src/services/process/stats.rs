use futures::{stream::select_all, StreamExt, TryStreamExt};
use ntex::web;

use bollard_next::container::StatsOptions;
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::{ProcessStats, ProcessStatsQuery};

use crate::{
  models::{ProcessDb, SystemState},
  utils,
};

/// Get stats of all processes of given kind and name (cargo, job, vm)
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{kind}/{name}/stats",
  params(
    ("kind" = String, Path, description = "Kind of process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process group", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
    ("stream" = Option<bool>, Query, description = "Return a stream of stats"),
    ("one_shot" = Option<bool>, Query, description = "Return stats only once"),
  ),
  responses(
    (status = 200, description = "Process stats", content_type = "application/vdn.nanocl.raw-stream", body = ProcessStats),
    (status = 404, description = "Process does not exist"),
  ),
))]
#[web::get("/processes/{kind}/{name}/stats")]
pub async fn stats_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessStatsQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  let opts: StatsOptions = qs.clone().into();
  let processes =
    ProcessDb::read_by_kind_key(&kind_key, None, &state.inner.pool).await?;
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
