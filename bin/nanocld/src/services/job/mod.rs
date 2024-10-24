use ntex::web;

pub mod count;
pub mod create;
pub mod delete;
pub mod inspect;
pub mod list;

pub use count::*;
pub use create::*;
pub use delete::*;
pub use inspect::*;
pub use list::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_job);
  config.service(create_job);
  config.service(delete_job);
  config.service(inspect_job);
  config.service(count_job);
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::job::{Job, JobSummary};
  use ntex::http;

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/jobs";

  #[ntex::test]
  async fn list_jobs() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let mut response = client.get(ENDPOINT).send().await.unwrap();
    test_status_code!(response.status(), http::StatusCode::OK, "list jobs");
    let _ = response.json::<Vec<JobSummary>>().await.unwrap();
  }

  #[ntex::test]
  async fn wait_not_found() {
    let system = gen_default_test_system().await;
    let client = system.client;
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
    let system = gen_default_test_system().await;
    let client = system.client;
    let state: &str = include_str!("../../../../../examples/job_example.yml");
    let yaml: serde_yaml::Value = serde_yaml::from_str(state).unwrap();
    let job_spec = &yaml["Jobs"][0];
    let mut res = client
      .send_post(ENDPOINT, Some(job_spec.clone()), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::CREATED, "create job");
    let job = res.json::<Job>().await.unwrap();
    let job_endpoint = format!("{ENDPOINT}/{}", &job.name);
    let mut res = client.get(ENDPOINT).send().await.unwrap();
    let _ = res.json::<Vec<JobSummary>>().await.unwrap();
    let res = client
      .send_get(
        &format!("/processes/job/{}/wait", &job.name),
        Some(&serde_json::json!({
          "condition": "yoloh"
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
    let res = client
      .send_get(&format!("{job_endpoint}/inspect"), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      format!("inspect job {}", &job.name)
    );
    let _ = client.send_delete(&job_endpoint, None::<String>).await;
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }
}
