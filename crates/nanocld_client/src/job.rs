use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::{
  generic::GenericFilter,
  job::{Job, JobInspect, JobPartial, JobSummary},
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

  /// Create a job from a [JobPartial](JobPartial)
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
}

#[cfg(test)]
mod tests {
  use futures::StreamExt;
  use bollard_next::container::Config;

  use crate::ConnectOpts;

  use super::*;

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
    assert_eq!(job.name, "my_test_job");
    let mut stream = client.wait_process("job", &job.name, None).await.unwrap();
    client.start_process("job", &job.name, None).await.unwrap();
    while let Some(Ok(_)) = stream.next().await {}
    let job = client.inspect_job(&job.name).await.unwrap();
    client.delete_job(&job.spec.name).await.unwrap();
  }
}
