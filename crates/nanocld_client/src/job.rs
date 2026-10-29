use nanocl_error::http_client::{HttpClientError, HttpClientResult};
use nanocl_error::io::{FromIo, IoError};

use nanocl_stubs::{
  generic::GenericFilter,
  job::{
    Job, JobDeleteQuery, JobHistoryQuery, JobInspect, JobPartial, JobRun,
    JobRunInspect, JobSpec, JobSummary,
  },
};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for jobs
  const JOB_PATH: &'static str = "/jobs";

  /// List existing jobs in the system
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_job().await;
  /// ```
  pub async fn list_job(
    &self,
    query: Option<&GenericFilter>,
  ) -> HttpClientResult<Vec<JobSummary>> {
    let query = Self::convert_query(query)?;
    let res = self.send_get(Self::JOB_PATH, Some(query)).await?;
    Self::res_json(res).await
  }

  /// Get information about a job by it's name
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.inspect_job("my_job").await;
  /// ```
  pub async fn inspect_job(&self, name: &str) -> HttpClientResult<JobInspect> {
    let res = self
      .send_get(
        &format!("{}/{name}/inspect", Self::JOB_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// List retained configuration revisions, including revisions of deleted Jobs.
  pub async fn list_history_job(
    &self,
    name: &str,
    query: Option<&JobHistoryQuery>,
  ) -> HttpClientResult<Vec<JobSpec>> {
    let res = self
      .send_get(&format!("{}/{name}/histories", Self::JOB_PATH), query)
      .await?;
    Self::res_json(res).await
  }

  /// Inspect an immutable configuration revision belonging to the named Job.
  /// This does not require a live Job or a run using the revision.
  pub async fn inspect_history_job(
    &self,
    name: &str,
    spec: &str,
  ) -> HttpClientResult<JobSpec> {
    let res = self
      .send_get(
        &format!("{}/{name}/histories/{spec}", Self::JOB_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// Restore a saved configuration without starting a Job execution.
  /// The saved schedule, if present, applies to future scheduled executions.
  /// Waits for running containers to stop without a response-header deadline.
  /// If cancelled or the response is lost, inspect the Job before retrying:
  /// the daemon retains ownership of the replacement after disconnection.
  pub async fn revert_job(
    &self,
    name: &str,
    id: &str,
  ) -> HttpClientResult<Job> {
    let id = id.parse::<uuid::Uuid>().map_err(|err| {
      IoError::invalid_input("Job history ID", &err.to_string())
    })?;
    // The number of containers and their stop grace have no shared upper
    // bound. Keep connect/payload deadlines, but allow the synchronous
    // replacement to finish before sending its response head. Never retry it.
    let mut response = self
      .patch(&format!("{}/{name}/histories/{id}/revert", Self::JOB_PATH))
      .await?
      .timeout(ntex::time::Millis::ZERO)
      .send()
      .await
      .map_err(|error| {
        Self::job_revert_outcome_error(self.send_error(error))
      })?;
    let status = response.status();
    crate::error::is_api_error(&mut response, &status)
      .await
      .map_err(Self::job_revert_outcome_error)?;
    Self::res_json(response)
      .await
      .map_err(Self::job_revert_outcome_error)
  }

  fn job_revert_outcome_error(error: HttpClientError) -> HttpClientError {
    match error {
      HttpClientError::IoError(error) => {
        HttpClientError::IoError(error.map_err_context(
          || "Job revert outcome is unknown; inspect the Job before retrying",
        ))
      }
      error => error,
    }
  }

  /// List retained executions, including executions of deleted Job incarnations.
  pub async fn list_runs_job(
    &self,
    name: &str,
    query: Option<&JobHistoryQuery>,
  ) -> HttpClientResult<Vec<JobRun>> {
    let res = self
      .send_get(&format!("{}/{name}/runs", Self::JOB_PATH), query)
      .await?;
    Self::res_json(res).await
  }

  /// Inspect a retained run, its pinned definition, operation, and ordered steps.
  /// This does not require a live Job or retained Docker containers/logs.
  pub async fn inspect_run_job(
    &self,
    name: &str,
    run: &str,
  ) -> HttpClientResult<JobRunInspect> {
    let res = self
      .send_get(
        &format!("{}/{name}/runs/{run}", Self::JOB_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// Create a job from a [JobPartial](JobPartial)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocld_client::bollard_next::models::ContainerCreateBody as Config;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.create_job(&JobPartial {
  ///  name: "my_job".to_string(),
  ///  containers: vec![
  ///   Config {
  ///     image: Some("alpine:latest".to_string()),
  ///     cmd: Some(vec!["echo".to_string(), "Hello world".to_string()]),
  ///   }
  ///  ],
  /// }).await;
  /// ```
  pub async fn create_job(&self, job: &JobPartial) -> HttpClientResult<Job> {
    let res = self
      .send_post(Self::JOB_PATH, Some(job.clone()), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// Delete a job by it's name
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_job("my_job").await;
  /// ```
  pub async fn delete_job(&self, name: &str) -> HttpClientResult<()> {
    self
      .send_delete(&format!("{}/{name}", Self::JOB_PATH), None::<String>)
      .await?;
    Ok(())
  }

  /// Delete only if the live Job still has the inspected incarnation and run.
  /// A missing run requires that this incarnation has not accepted a run.
  pub async fn delete_job_if_current(
    &self,
    name: &str,
    incarnation: uuid::Uuid,
    run: Option<uuid::Uuid>,
  ) -> HttpClientResult<()> {
    self
      .send_delete(
        &format!("{}/{name}", Self::JOB_PATH),
        Some(JobDeleteQuery {
          incarnation: Some(incarnation),
          run,
        }),
      )
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use bollard_next::models::ContainerCreateBody as Config;
  use futures::StreamExt;
  use ntex::web;

  use crate::ConnectOpts;

  use super::*;

  #[test]
  fn job_revert_unknown_outcome_preserves_transport_and_api_errors() {
    let error = NanocldClient::job_revert_outcome_error(
      IoError::without_context(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "response was lost",
      ))
      .into(),
    );
    assert!(
      error
        .to_string()
        .contains("inspect the Job before retrying")
    );
    assert!(error.to_string().contains("Response was lost"));
    assert!(matches!(error, HttpClientError::IoError(error)
      if error.inner.kind() == std::io::ErrorKind::TimedOut
        && error.inner.to_string() == "response was lost"));
    let error = NanocldClient::job_revert_outcome_error(
      nanocl_error::http::HttpError::conflict("identity changed").into(),
    );
    assert!(matches!(error, HttpClientError::HttpError(error)
      if error.status == ntex::http::StatusCode::CONFLICT
        && error.msg == "identity changed"));
  }

  #[ntex::test]
  async fn revert_job_lost_response_reports_unknown_outcome_without_retry() {
    let (client, requests, _server) = crate::process::receipt_test::client(
      ntex::http::StatusCode::OK,
      None,
      None,
    );
    let error = client
      .revert_job("backup", &uuid::Uuid::from_u128(42).to_string())
      .await
      .unwrap_err();
    assert!(error.to_string().contains("Job revert outcome is unknown"));
    assert!(
      error
        .to_string()
        .contains("inspect the Job before retrying")
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
  }

  #[ntex::test]
  async fn revert_job_restores_configuration_without_starting_an_execution() {
    use ntex::http::StatusCode;
    let revision = uuid::Uuid::from_u128(42);
    let job = Job {
      incarnation_key: uuid::Uuid::from_u128(43),
      spec_key: uuid::Uuid::from_u128(44),
      spec: JobSpec {
        key: uuid::Uuid::from_u128(44),
        job_key: "backup".to_owned(),
        name: "backup".to_owned(),
        schedule: Some("0 0 * * *".to_owned()),
        ..Default::default()
      },
      ..Default::default()
    };
    let (client, requests, _server) =
      crate::process::receipt_test::client_with_body(
        StatusCode::OK,
        None,
        None,
        Some(serde_json::to_value(&job).unwrap()),
      );
    assert_eq!(
      client
        .revert_job("backup", &revision.to_string())
        .await
        .unwrap(),
      job,
    );
    assert!(
      client
        .revert_job("backup", "invalid-revision")
        .await
        .is_err()
    );
    let recorded = requests.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].method, "PATCH");
    assert_eq!(
      recorded[0].path,
      format!(
        "/v{}/jobs/backup/histories/{revision}/revert",
        crate::NANOCLD_DEFAULT_VERSION,
      ),
    );
    assert!(recorded[0].query.is_empty());
    assert!(recorded[0].body.is_empty());
  }

  #[ntex::test]
  async fn revert_job_propagates_missing_history_and_conflict() {
    use ntex::http::StatusCode;
    for status in [StatusCode::NOT_FOUND, StatusCode::CONFLICT] {
      let (client, requests, _server) =
        crate::process::receipt_test::client(status, None, None);
      let error = client
        .revert_job("backup", &uuid::Uuid::from_u128(42).to_string())
        .await
        .unwrap_err();
      assert!(matches!(error,
        nanocl_error::http_client::HttpClientError::HttpError(error)
          if error.status == status));
      assert_eq!(requests.lock().unwrap().len(), 1);
    }
  }

  async fn delete_exact_job_fixture(
    request: web::HttpRequest,
    query: web::types::Query<JobDeleteQuery>,
  ) -> web::HttpResponse {
    request
      .app_state::<Arc<Mutex<Vec<String>>>>()
      .unwrap()
      .lock()
      .unwrap()
      .push(request.query_string().to_owned());
    if query.run.is_some() {
      web::HttpResponse::Conflict().json(&serde_json::json!({
        "msg": "Job run changed before deletion",
      }))
    } else {
      web::HttpResponse::Accepted().build()
    }
  }

  #[ntex::test]
  async fn delete_job_if_current_encodes_optional_run_and_propagates_conflict()
  {
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let server_requests = requests.clone();
    let server = web::test::server(async move |_| {
      web::App::new()
        .with_config(
          web::WebAppConfig::new().set_state(server_requests.clone()),
        )
        .service(
          web::resource(format!(
            "/v{}/jobs/backup",
            crate::NANOCLD_DEFAULT_VERSION
          ))
          .route(web::delete().to(delete_exact_job_fixture)),
        )
    });
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: format!("http://localhost:{}", server.addr().port()),
      ..Default::default()
    })
    .unwrap();
    let incarnation = uuid::Uuid::from_u128(1);
    let run = uuid::Uuid::from_u128(2);
    client
      .delete_job_if_current("backup", incarnation, None)
      .await
      .unwrap();
    let error = client
      .delete_job_if_current("backup", incarnation, Some(run))
      .await
      .unwrap_err();
    assert!(matches!(error,
      nanocl_error::http_client::HttpClientError::HttpError(error)
        if error.status == ntex::http::StatusCode::CONFLICT));
    assert_eq!(
      *requests.lock().unwrap(),
      [
        format!("incarnation={incarnation}"),
        format!("incarnation={incarnation}&run={run}"),
      ]
    );
  }

  #[ntex::test]
  async fn list_job() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    client.list_job(None).await.unwrap();
  }

  #[ntex::test]
  async fn basic() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    let job = client
      .create_job(&JobPartial {
        name: "my_test_job".to_owned(),
        containers: vec![Config {
          image: Some("alpine:latest".to_owned()),
          cmd: Some(vec!["echo".to_owned(), "Hello world".to_owned()]),
          ..Default::default()
        }],
        schedule: None,
        secrets: None,
        metadata: None,
        ttl: None,
        image_pull_secret: None,
        image_pull_policy: None,
      })
      .await
      .unwrap();
    assert_eq!(job.spec.name, "my_test_job");
    let mut stream = client
      .wait_process("job", &job.spec.name, None)
      .await
      .unwrap();
    client.start_process("job", &job.spec.name).await.unwrap();
    while let Some(Ok(_)) = stream.next().await {}
    let job = client.inspect_job(&job.spec.name).await.unwrap();
    client.delete_job(&job.spec.name).await.unwrap();
  }
}
