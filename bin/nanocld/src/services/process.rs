use ntex::web;
use futures_util::stream::select_all;
use nanocl_stubs::process::{ProcessLogQuery, ProcessOutputLog};
use futures_util::{StreamExt, TryStreamExt};

use nanocl_error::http::{HttpError, HttpResult};

use bollard_next::container::LogsOptions;

use crate::utils;
use crate::models::{DaemonState, ProcessDb};

/// Get logs of an instance
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{kind}/{name}/logs",
  params(
    ("kind" = String, Path, description = "Type of the instance"),
    ("name" = String, Path, description = "Name of the instance"),
    ("namespace" = Option<String>, Query, description = "Namespace of the instance"),
    ("since" = Option<i64>, Query, description = "Only logs returned since timestamp"),
    ("until" = Option<i64>, Query, description = "Only logs returned until timestamp"),
    ("timestamps" = Option<bool>, Query, description = "Add timestamps to every log line"),
    ("follow" = Option<bool>, Query, description = "Boolean to return a stream or not"),
    ("tail" = Option<String>, Query, description = "Only return the n last (integer) or all (\"all\") logs"),
  ),
  responses(
    (status = 200, description = "Instance logs", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Instance not exists"),
  ),
))]
#[web::get("/processes/{kind}/{name}/logs")]
async fn get_process_logs(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessLogQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind_id = match kind.as_str() {
    "job" => name,
    "cargo" | "vm" => {
      let namespace = utils::key::resolve_nsp(&qs.namespace);
      utils::key::gen_key(&namespace, &name)
    }
    _ => return Err(HttpError::bad_request(format!("Invalid kind: {kind}"))),
  };
  let instances = ProcessDb::find_by_kind_id(&kind_id, &state.pool).await?;
  log::debug!("instance::get_instances_logs instances: {instances:#?}");
  let options: LogsOptions<String> = qs.into_inner().into();
  let futures = instances
    .into_iter()
    .map(|instance| {
      state
        .docker_api
        .logs(
          &instance.data.id.unwrap_or_default(),
          Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            ..options.clone()
          }),
        )
        .map(move |elem| match elem {
          Err(err) => Err(err),
          Ok(elem) => {
            log::debug!("{:#?} {elem}", &instance.name);
            Ok(ProcessOutputLog {
              name: instance.name.clone(),
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

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(get_process_logs);
}
