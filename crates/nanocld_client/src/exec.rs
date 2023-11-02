use ntex::channel::mpsc;
use bollard_next::service::ExecInspectResponse;
use bollard_next::exec::{CreateExecResults, StartExecOptions};

use nanocl_error::http::HttpError;
use nanocl_error::http_client::HttpClientError;

use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::cargo::{CreateExecOptions, OutputLog};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for exec commands
  const EXEC_PATH: &str = "/exec";

  /// ## Create exec command inside a cargo
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to exec the command in
  /// * [exec](CreateExecOptions) - The config for the exec command
  /// * [namespace](Option) - The [namespace](str) where belong the cargo
  ///
  /// ## Returns
  ///
  /// * [Result](Result) - The result of the operation
  ///   * [Ok](Ok) - [Created exec](CreateExecResults) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocld_client::models::cargo_config::CreateExecOptions;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let exec = CreateExecOptions {
  ///  cmd: vec!["echo".into(), "hello".into()],
  /// ..Default::default()
  /// };
  /// let result = client.create_exec("my-cargo", exec, None).await.unwrap();
  /// println!("{}", result);
  /// ```
  ///
  pub async fn create_exec(
    &self,
    name: &str,
    exec: &CreateExecOptions,
    namespace: Option<&str>,
  ) -> Result<CreateExecResults, HttpClientError> {
    let res = self
      .send_post(
        &format!("/cargoes/{name}/exec"),
        Some(exec),
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Inspect exec
  ///
  /// Inspect an exec command inside a cargo instance.
  ///
  /// ## Arguments
  ///
  /// * [id](str) - Id of command to inspect
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - [Info](ExecInspectResponse) of the exec command if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocld_client::models::cargo_config::{CreateExecOptions, StartExecOptions};
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let exec = CreateExecOptions {
  ///   cmd: Some(vec!["echo".into(), "hello".into()]),
  ///   ..Default::default()
  /// };
  /// let result = client.create_exec("my-cargo", exec, None).await.unwrap();
  /// let mut rx = client
  ///   .start_exec(&result.id, StartExecOptions::default())
  ///   .await
  ///   .unwrap();
  /// while let Some(_out) = rx.next().await {}
  ///
  /// client.inspect_exec(&result.id).await.unwrap();
  /// let result = client.inspect_exec("my-cargo", exec, None).await.unwrap();
  /// println!("{}", result);
  /// ```
  ///
  pub async fn inspect_exec(
    &self,
    id: &str,
  ) -> Result<ExecInspectResponse, HttpClientError> {
    let res = self
      .send_get(&format!("{}/{id}/cargo/inspect", Self::EXEC_PATH), Some(()))
      .await?;
    Self::res_json(res).await
  }

  /// ## Run an command inside a cargo
  ///
  /// ## Arguments
  ///
  /// * [id](str) - Id of command to run
  /// * [exec](CreateExecOptions) - The config for the exec command
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - [Receiver](mpsc::Receiver) of [output log](OutputLog) if operation was successful
  ///   * [Err](Err) - [Http client error](HttpClientError) if operation failed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use futures::StreamExt;
  /// use nanocld_client::NanocldClient;
  /// use nanocld_client::models::cargo_config::CreateExecOptions;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let exec = CreateExecOptions {
  ///  cmd: vec!["echo".into(), "hello".into()],
  /// ..Default::default()
  /// };
  /// let result = client.create_exec("my-cargo", exec, None).await.unwrap();
  /// let mut rx = client.start_exec(&result.id, StartExec::default(), None).await.unwrap();
  /// while let Some(output) = rx.next().await {
  ///  println!("{}", output);
  /// };
  /// ```
  ///
  pub async fn start_exec(
    &self,
    id: &str,
    exec: &StartExecOptions,
  ) -> Result<mpsc::Receiver<Result<OutputLog, HttpError>>, HttpClientError> {
    let res = self
      .send_post(
        &format!("{}/{id}/cargo/start", &Self::EXEC_PATH),
        Some(exec),
        Some(()),
      )
      .await?;
    Ok(Self::res_stream(res).await)
  }
}

#[cfg(test)]
mod tests {
  use bollard_next::exec::{CreateExecOptions, StartExecOptions};
  use futures::StreamExt;

  use crate::NanocldClient;

  #[ntex::test]
  async fn exec_cargo() {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let exec = CreateExecOptions {
      cmd: Some(vec!["echo".into(), "hello".into()]),
      ..Default::default()
    };
    let result = client
      .create_exec("nstore", &exec, Some("system"))
      .await
      .unwrap();
    let mut rx = client
      .start_exec(&result.id, &StartExecOptions::default())
      .await
      .unwrap();
    while let Some(_out) = rx.next().await {}
    client.inspect_exec(&result.id).await.unwrap();
  }
}
