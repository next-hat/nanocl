use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpResult;
use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::job::{
  Job, JobLogOutput, JobWaitQuery, JobWaitResponse, JobPartial, JobInspect,
  JobSummary,
};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for jobs
  const JOB_PATH: &'static str = "/jobs";

  /// ## List jobs
  ///
  /// List existing jobs in the system
  ///
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Vec](Vec) of [JobSummary](JobSummary)
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
  pub async fn list_job(&self) -> HttpClientResult<Vec<JobSummary>> {
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
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [JobInspect](JobInspect)
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
  pub async fn inspect_job(&self, name: &str) -> HttpClientResult<JobInspect> {
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
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Receiver](Receiver) of [JobLogOutput](JobLogOutput)
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
  ) -> HttpClientResult<Receiver<HttpResult<JobLogOutput>>> {
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
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Job](Job)
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
  pub async fn create_job(&self, job: &JobPartial) -> HttpClientResult<Job> {
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
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.start_job("my_job").await;
  /// ```
  ///
  pub async fn start_job(&self, name: &str) -> HttpClientResult<()> {
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
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Receiver](Receiver) of [JobWaitResponse](JobWaitResponse)
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
  ) -> HttpClientResult<Receiver<HttpResult<JobWaitResponse>>> {
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
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_job("my_job").await;
  /// ```
  ///
  pub async fn delete_job(&self, name: &str) -> HttpClientResult<()> {
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
        schedule: None,
        secrets: None,
        metadata: None,
        ttl: None,
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
