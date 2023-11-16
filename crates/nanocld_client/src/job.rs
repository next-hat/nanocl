use ntex::channel::mpsc::Receiver;

use nanocl_stubs::job::{
  Job, JobLogOutput, JobWaitQuery, JobWaitResponse, JobPartial, JobInspect,
  JobSummary,
};
use nanocl_error::http::HttpError;
use nanocl_error::http_client::HttpClientError;

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for jobs
  const JOB_PATH: &'static str = "/jobs";

  /// ## List jobs
  ///
  /// List existing jobs in the system
  ///
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Vector](Vec) of [job](Job) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_job().await;
  /// ```
  ///
  pub async fn list_job(&self) -> Result<Vec<JobSummary>, HttpClientError> {
    let res = self.send_get(Self::JOB_PATH, None::<String>).await?;
    Self::res_json(res).await
  }

  /// ## Inspect a job
  ///
  /// Get information about a job by it's name
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the job to inspect
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Job inspect](JobInspect) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.inspect_job("my_job").await;
  /// ```
  ///
  pub async fn inspect_job(
    &self,
    name: &str,
  ) -> Result<JobInspect, HttpClientError> {
    let res = self
      .send_get(
        &format!("{}/{name}/inspect", Self::JOB_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Logs a job
  ///
  /// Get logs of a job by it's name
  /// The logs are streamed as a [Receiver](Receiver) of [output log](OutputLog)
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the job to get the logs
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Receiver](Receiver) of [Result](Result) of [JobLogOutput](JobLogOutput) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let stream = client.logs_job("my_job").await.unwrap();
  /// ```
  ///
  pub async fn logs_job(
    &self,
    name: &str,
  ) -> Result<Receiver<Result<JobLogOutput, HttpError>>, HttpClientError> {
    let res = self
      .send_get(&format!("{}/{name}/logs", Self::JOB_PATH), None::<String>)
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Create a job
  ///
  /// Create a job from a [JobPartial](JobPartial)
  ///
  /// ## Arguments
  ///
  /// * [job](JobPartial) - The job to create
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Job](Job) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocld_client::bollard_next::container::Config;
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
  ///
  pub async fn create_job(
    &self,
    job: &JobPartial,
  ) -> Result<Job, HttpClientError> {
    let res = self
      .send_post(Self::JOB_PATH, Some(job.clone()), None::<String>)
      .await?;
    Self::res_json(res).await
  }

  /// ## Start a job
  ///
  /// Start a job by it's name
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the job to start
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [()] if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.start_job("my_job").await;
  /// ```
  ///
  pub async fn start_job(&self, name: &str) -> Result<(), HttpClientError> {
    self
      .send_post(
        &format!("{}/{name}/start", Self::JOB_PATH),
        None::<String>,
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// ## Wait a job
  ///
  /// A [Receiver](Receiver) stream of [ContainerWaitResponse](ContainerWaitResponse) is
  /// returned, data are sent when container end
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the job to get the logs
  /// * [query](Option) - Optional [query](JobWaitQuery)
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Receiver](Receiver) of [Result](Result) of [JobWaitResponse](JobWaitResponse) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let stream = client.wait_job("my_job", None).await.unwrap();
  /// ```
  ///
  pub async fn wait_job(
    &self,
    name: &str,
    query: Option<&JobWaitQuery>,
  ) -> Result<Receiver<Result<JobWaitResponse, HttpError>>, HttpClientError> {
    let res = self
      .send_get(&format!("{}/{name}/wait", Self::JOB_PATH), query)
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Delete a job
  ///
  /// Delete a job by it's name
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the job to delete
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - If operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_job("my_job").await;
  /// ```
  ///
  pub async fn delete_job(&self, name: &str) -> Result<(), HttpClientError> {
    self
      .send_delete(&format!("{}/{name}", Self::JOB_PATH), None::<String>)
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use futures::StreamExt;
  use bollard_next::container::Config;

  use super::*;

  #[ntex::test]
  async fn list_job() {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    client.list_job().await.unwrap();
  }

  #[ntex::test]
  async fn basic() {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let job = client
      .create_job(&JobPartial {
        name: "my_test_job".to_owned(),
        containers: vec![Config {
          image: Some("alpine:latest".to_owned()),
          cmd: Some(vec!["echo".to_owned(), "Hello world".to_owned()]),
          ..Default::default()
        }],
        secrets: None,
        metadata: None,
      })
      .await
      .unwrap();
    assert_eq!(job.name, "my_test_job");
    let mut stream = client.wait_job(&job.name, None).await.unwrap();
    client.start_job(&job.name).await.unwrap();
    while let Some(Ok(_)) = stream.next().await {}
    let mut stream = client.logs_job(&job.name).await.unwrap();
    while let Some(Ok(_)) = stream.next().await {}
    let job = client.inspect_job(&job.name).await.unwrap();
    client.delete_job(&job.name).await.unwrap();
  }
}
