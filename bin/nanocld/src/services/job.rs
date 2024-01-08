use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::job::JobPartial;

use crate::{
  utils,
  objects::generic::*,
  models::{SystemState, JobDb},
};

/// List jobs
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs",
  responses(
    (status = 200, description = "List of jobs", body = [JobSummary]),
  ),
))]
#[web::get("/jobs")]
pub async fn list_job(
  state: web::types::State<SystemState>,
  _version: web::types::Path<String>,
) -> HttpResult<web::HttpResponse> {
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
    (status = 201, description = "Job created", body = Job),
  ),
))]
#[web::post("/jobs")]
pub async fn create_job(
  state: web::types::State<SystemState>,
  _version: web::types::Path<String>,
  payload: web::types::Json<JobPartial>,
) -> HttpResult<web::HttpResponse> {
  let job = JobDb::create_obj(&payload, &state).await?;
  Ok(web::HttpResponse::Created().json(&job))
}

/// Delete a job
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Jobs",
  path = "/jobs/{name}",
  params(
    ("name" = String, Path, description = "Name of the job"),
  ),
  responses(
    (status = 202, description = "Job deleted"),
    (status = 404, description = "Job does not exist"),
  ),
))]
#[web::delete("/jobs/{name}")]
pub async fn delete_job(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  JobDb::del_obj_by_pk(&path.1, &(), &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Inspect a job
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Jobs",
  path = "/jobs/{name}/inspect",
  params(
    ("name" = String, Path, description = "Name of the job"),
  ),
  responses(
    (status = 200, description = "Job details", body = JobInspect),
  ),
))]
#[web::get("/jobs/{name}/inspect")]
pub async fn inspect_job(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let job = JobDb::inspect_obj_by_pk(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&job))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_job);
  config.service(create_job);
  config.service(delete_job);
  config.service(inspect_job);
}

#[cfg(test)]
mod tests {
  use ntex::http;
  use futures_util::{StreamExt, TryStreamExt};
  use nanocl_stubs::{
    job::{Job, JobSummary},
    process::ProcessWaitResponse,
  };

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/jobs";

  #[ntex::test]
  async fn list_jobs() {
    let client = gen_default_test_client().await;
    let mut response = client.get(ENDPOINT).send().await.unwrap();
    test_status_code!(response.status(), http::StatusCode::OK, "list jobs");
    let _ = response.json::<Vec<JobSummary>>().await.unwrap();
  }

  #[ntex::test]
  async fn wait_not_found() {
    let client = gen_default_test_client().await;
    let wait_res = client
      .send_get(
        &format!("{ENDPOINT}/test/wait"),
        Some(&serde_json::json!({
          "condition": "yoloh"
        })),
      )
      .await;
    test_status_code!(
      wait_res.status(),
      http::StatusCode::NOT_FOUND,
      "wait job not found"
    );
  }

  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    let state: &str = include_str!("../../../../examples/job_example.yml");
    let yaml: serde_yaml::Value = serde_yaml::from_str(state).unwrap();
    let job_spec = &yaml["Jobs"][0];
    let mut res = client
      .send_post(ENDPOINT, Some(job_spec.clone()), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::CREATED, "create job");
    let job = res.json::<Job>().await.unwrap();
    let job_endpoint = format!("{ENDPOINT}/{}", &job.name);
    let wait_res = client
      .send_get(
        &format!("/processes/job/{}/wait", &job.name),
        None::<String>,
      )
      .await;
    test_status_code!(
      wait_res.status(),
      http::StatusCode::OK,
      format!("wait job {}", &job.name)
    );
    let mut res = client.get(ENDPOINT).send().await.unwrap();
    let _ = res.json::<Vec<JobSummary>>().await.unwrap();
    let res = client
      .send_get(
        &format!("{job_endpoint}/wait"),
        Some(&serde_json::json!({
          "Condition": "yoloh"
        })),
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "wait job bad condition"
    );
    client
      .send_post(
        &format!("/processes/job/{}/start", &job.name),
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
        serde_json::from_slice::<ProcessWaitResponse>(&wait_response).unwrap();
      assert_eq!(response.status_code, 0);
    }
    let res = client
      .send_get(&format!("{job_endpoint}/inspect"), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      format!("inspect job {}", &job.name)
    );
    let _ = client.send_delete(&job_endpoint, None::<String>).await;
  }
}
