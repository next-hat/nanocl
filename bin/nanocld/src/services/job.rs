use ntex::web;

use bollard_next::container::WaitContainerOptions;

use nanocl_error::http::HttpError;
use nanocl_stubs::job::{JobPartial, JobWaitQuery};

use crate::utils;
use crate::models::DaemonState;

/// List jobs
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs",
  responses(
    (status = 200, description = "List of jobs", body = [Job]),
  ),
))]
#[web::get("/jobs")]
pub(crate) async fn list_job(
  state: web::types::State<DaemonState>,
  _version: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpError> {
  let jobs = utils::job::list(&state).await?;
  Ok(web::HttpResponse::Ok().json(&jobs))
}

/// Create a job
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Jobs",
  path = "/jobs",
  request_body = JobPartial,
  responses(
    (status = 201, description = "Cargo created", body = Job),
  ),
))]
#[web::post("/jobs")]
pub(crate) async fn create_job(
  web::types::Json(payload): web::types::Json<JobPartial>,
  state: web::types::State<DaemonState>,
  _version: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpError> {
  let job = utils::job::create(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&job))
}

/// Start a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Jobs",
  path = "/jobs/{Name}/start",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
  ),
  responses(
    (status = 202, description = "Cargo started"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/jobs/{name}/start")]
pub(crate) async fn start_job(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  utils::job::start_by_name(&path.1, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Delete a job
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Jobs",
  path = "/jobs/{Name}",
  params(
    ("Name" = String, Path, description = "Name of the job"),
  ),
  responses(
    (status = 202, description = "Cargo deleted"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::delete("/jobs/{name}")]
pub(crate) async fn delete_job(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  utils::job::delete_by_name(&path.1, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Inspect a job
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{Name}/inspect",
  params(
    ("Name" = String, Path, description = "Name of the job"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the job"),
  ),
  responses(
    (status = 200, description = "Cargo details", body = CargoInspect),
  ),
))]
#[web::get("/jobs/{name}/inspect")]
async fn inspect_job(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let job = utils::job::inspect_by_name(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&job))
}

/// Get logs of a job
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{Name}/logs",
  responses(
    (status = 200, description = "Job logs", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Job does not exist"),
  ),
))]
#[web::get("/jobs/{name}/logs")]
async fn logs_job(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let stream = utils::job::logs_by_name(&path.1, &state).await?;
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(stream),
  )
}

/// Wait for a job to finish
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{Name}/wait",
  params(
    ("Name" = String, Path, description = "Name of the job instance usually `name` or `name-number`"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the job"),
  ),
  responses(
    (status = 200, description = "Cargo wait", content_type = "application/vdn.nanocl.raw-stream"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::get("/jobs/{name}/wait")]
async fn wait_job(
  web::types::Query(qs): web::types::Query<JobWaitQuery>,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let stream = utils::job::wait(
    &path.1,
    WaitContainerOptions {
      condition: qs.condition.unwrap_or_default(),
    },
    &state,
  )
  .await?;
  Ok(
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(stream),
  )
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_job);
  config.service(create_job);
  config.service(delete_job);
  config.service(inspect_job);
  config.service(logs_job);
  config.service(wait_job);
  config.service(start_job);
}

#[cfg(test)]
mod tests {
  use ntex::http;
  use futures_util::{StreamExt, TryStreamExt};
  use nanocl_stubs::job::{Job, JobWaitResponse};

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/jobs";

  #[ntex::test]
  async fn list_jobs() {
    let client = gen_default_test_client().await;
    let mut response = client.get(ENDPOINT).send().await.unwrap();
    test_status_code!(response.status(), http::StatusCode::OK, "list jobs");
    let _ = response.json::<Vec<Job>>().await.unwrap();
  }

  #[ntex::test]
  async fn wait_job() {
    let client = gen_default_test_client().await;
    let state: &str = include_str!("../../../../examples/job_example.yml");
    let yaml: serde_yaml::Value = serde_yaml::from_str(state).unwrap();
    let job_config = &yaml["Jobs"][0];
    let mut res = client
      .send_post(ENDPOINT, Some(job_config.clone()), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::CREATED, "create job");
    let job = res.json::<Job>().await.unwrap();
    let wait_res = client
      .send_get(&format!("{ENDPOINT}/{}/wait", &job.name), None::<String>)
      .await;
    test_status_code!(
      wait_res.status(),
      http::StatusCode::OK,
      format!("wait job {}", &job.name)
    );
    client
      .send_post(
        &format!("{ENDPOINT}/{}/start", &job.name),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      wait_res.status(),
      http::StatusCode::OK,
      format!("start job {}", &job.name)
    );
    let mut stream = wait_res.into_stream();
    while let Some(Ok(wait_response)) = stream.next().await {
      let response =
        serde_json::from_slice::<JobWaitResponse>(&wait_response).unwrap();
      assert_eq!(response.status_code, 0);
    }
    let _ = client
      .send_delete(&format!("{ENDPOINT}/{}", &job.name), None::<String>)
      .await;
  }
}
