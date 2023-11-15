use ntex::channel::mpsc::Receiver;

use nanocl_stubs::job::{Job, JobLogOutput};
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
  pub async fn list_job(&self) -> Result<Vec<Job>, HttpClientError> {
    let res = self.send_get(Self::JOB_PATH, None::<String>).await?;
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
  pub async fn logs_job(
    &self,
    name: &str,
  ) -> Result<Receiver<Result<JobLogOutput, HttpError>>, HttpClientError> {
    let res = self
      .send_get(&format!("{}/{name}/logs", Self::JOB_PATH), None::<String>)
      .await?;
    Ok(Self::res_stream(res).await)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_error::http_client::HttpClientError;

  #[ntex::test]
  async fn list_job() -> Result<(), HttpClientError> {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let res = client.list_job().await;
    assert!(res.is_ok());
    Ok(())
  }
}
