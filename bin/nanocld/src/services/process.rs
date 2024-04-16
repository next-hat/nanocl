use ntex::web;
use futures_util::{StreamExt, TryStreamExt, stream::select_all};

use nanocl_error::http::{HttpResult, HttpError};

use bollard_next::{
  container::{LogsOptions, WaitContainerOptions, StartContainerOptions},
  service::ContainerWaitExitError,
};
use nanocl_stubs::{
  generic::{GenericNspQuery, GenericFilter, GenericListQuery},
  process::{
    ProcessLogQuery, ProcessOutputLog, ProcessWaitQuery, ProcessWaitResponse,
  },
  cargo::CargoKillOptions,
};

use crate::{
  utils,
  repositories::generic::*,
  models::{SystemState, ProcessDb},
};

/// List process (Vm, Job, Cargo)
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"where\": { \"name\": { \"eq\": \"test\" } } }"),
  ),
  responses(
    (status = 200, description = "List of instances", body = [Process]),
  ),
))]
#[web::get("/processes")]
pub async fn list_process(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::try_from(qs.into_inner()).map_err(|err| {
    HttpError::bad_request(format!("Invalid query string: {err}"))
  })?;
  let processes = ProcessDb::transform_read_by(&filter, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&processes))
}

/// Get logs of processes for all instances of given kind and name (cargo, job, vm)
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
  ),
))]
#[web::get("/processes/{kind}/{name}/logs")]
async fn logs_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<ProcessLogQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  let processes = ProcessDb::read_by_kind_key(&kind_key, &state.pool).await?;
  log::debug!("process::logs_process: {kind_key}");
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

/// Get logs of processes for given kind and name
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/{name}/logs",
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
  ),
))]
#[web::get("/processes/{name}/logs")]
async fn logs_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<ProcessLogQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  log::debug!("process::logs_process: {name}");
  let options: LogsOptions<String> = qs.into_inner().into();
  let stream = state
    .docker_api
    .logs(
      &name,
      Some(LogsOptions::<String> {
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

/// Start process by it's pk
/// Internal endpoint used for multi node communication
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{pk}/start",
  params(
    ("pk" = String, Path, description = "Pk of the process", example = "1234567890"),
  ),
  responses(
    (status = 202, description = "Process instances started"),
  ),
))]
#[web::post("/processes/{pk}/start")]
pub async fn start_process_by_pk(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, pk) = path.into_inner();
  let process = ProcessDb::read_by_pk(&pk, &state.pool).await?;
  state
    .docker_api
    .start_container(&process.key, None::<StartContainerOptions<String>>)
    .await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Start processes of given kind and name
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
    (status = 202, description = "Process instances started"),
  ),
))]
#[web::post("/processes/{kind}/{name}/start")]
pub async fn start_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  utils::container::emit_starting(&kind_key, &kind, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Restart processes of given kind and name
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{name}/restart",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace where the process belongs is needed"),
  ),
  responses(
    (status = 202, description = "Process instances restarted"),
  ),
))]
#[web::post("/processes/{kind}/{name}/restart")]
pub async fn restart_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  let kind_pk = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  utils::container::restart_instances(&kind_pk, &kind, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Stop a processes of given kind and name
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
    (status = 202, description = "Process instances stopped"),
  ),
))]
#[web::post("/processes/{kind}/{name}/stop")]
pub async fn stop_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  let kind_key = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  utils::container::emit_stopping(&kind_key, &kind, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Send a signal to processes of given kind and name
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  request_body = CargoKillOptions,
  path = "/processes/{kind}/{name}/kill",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace where the process belongs is needed"),
  ),
  responses(
    (status = 200, description = "Process instances killed"),
  ),
))]
#[web::post("/processes/{kind}/{name}/kill")]
pub async fn kill_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  payload: web::types::Json<CargoKillOptions>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  let kind_pk = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  utils::container::kill_by_kind_key(&kind_pk, &payload, &state).await?;
  Ok(web::HttpResponse::Ok().into())
}

/// Wait for a job to finish
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
pub async fn wait_process(
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
  let processes = ProcessDb::read_by_kind_key(&kind_pk, &state.pool).await?;
  let mut streams = Vec::new();
  for process in processes {
    let options = Some(opts.clone());
    let stream = state.docker_api.wait_container(&process.key, options).map(
      move |wait_result| match wait_result {
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
      },
    );
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

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_process);
  config.service(logs_processes);
  config.service(logs_process);
  config.service(restart_process);
  config.service(start_process);
  config.service(stop_process);
  config.service(kill_process);
  config.service(wait_process);
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use crate::utils::tests::*;

  use nanocl_stubs::{
    process::Process,
    generic::{GenericFilter, GenericClause, GenericListQuery},
  };

  #[ntex::test]
  async fn basic_list() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let mut res = client.send_get("/processes", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let _ = res.json::<Vec<Process>>().await.unwrap();
  }

  #[ntex::test]
  async fn list_by() {
    let system = gen_default_test_system().await;
    let client = system.client;
    // Filter by namespace
    let filter = GenericFilter::new().r#where(
      "data",
      GenericClause::Contains(serde_json::json!({
        "Config": {
          "Labels": {
            "io.nanocl.n": "system",
          }
        }
      })),
    );
    let qs = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get("/processes", Some(qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let items: Vec<Process> = res.json::<Vec<Process>>().await.unwrap();
    assert!(items.iter().any(|i| i.name == "nstore.system.c"));
    // Filter by limit and offset
    let filter = GenericFilter::new().limit(1).offset(1);
    let qs = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get("/processes", Some(qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let items: Vec<Process> = res.json::<Vec<Process>>().await.unwrap();
    assert_eq!(items.len(), 1);
    // Filter by name and kind
    let filter = GenericFilter::new()
      .r#where("name", GenericClause::Like("nstore%".to_owned()))
      .r#where("kind", GenericClause::Eq("cargo".to_owned()));
    let qs = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get("/processes", Some(qs)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let items: Vec<Process> = res.json::<Vec<Process>>().await.unwrap();
    assert!(items.iter().any(|i| i.name == "nstore.system.c"));
  }
}
