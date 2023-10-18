use bollard_next::exec::{CreateExecResults, StartExecOptions};
use bollard_next::service::ExecInspectResponse;
use ntex::channel::mpsc;

use nanocl_utils::http_error::HttpError;
use nanocl_utils::http_client_error::HttpClientError;

use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::cargo::{CreateExecOptions, OutputLog};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Create exec command inside a cargo
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to exec the command in
  /// * [exec](CreateExecOptions) - The config for the exec command
  /// * [namespace](Option<String>) - The namespace where belong the cargo
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///  * [Ok](Ok) - The created exec command
  /// * [Err](HttpClientError) - The command could not be executed
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
    exec: CreateExecOptions,
    namespace: Option<String>,
  ) -> Result<CreateExecResults, HttpClientError> {
    let res = self
      .send_post(
        format!("/{}/cargoes/{name}/exec", &self.version),
        Some(exec),
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Ok(Self::res_json(res).await.unwrap())
  }

  /// ## Inspect an exec command inside a cargo
  ///
  /// ## Arguments
  ///
  /// * [id](str) - Id of command to inspect
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///  * [Ok](Ok) - Infos of the inspected command
  /// * [Err](HttpClientError) - The command could not be executed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocld_client::models::cargo_config::{CreateExecOptions, StartExecOptions};
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  ///
  ///  let exec = CreateExecOptions {
  ///    cmd: Some(vec!["echo".into(), "hello".into()]),
  ///    ..Default::default()
  ///  };
  ///
  ///  let result = client.create_exec("my-cargo", exec, None).await.unwrap();
  ///
  ///  let mut rx = client
  ///    .start_exec(&result.id, StartExecOptions::default())
  ///    .await
  ///    .unwrap();
  ///  while let Some(_out) = rx.next().await {}
  ///
  ///  client.inspect_exec(&result.id).await.unwrap();
  ///  let result = client.inspect_exec("my-cargo", exec, None).await.unwrap();
  ///  println!("{}", result);
  /// ```
  ///
  pub async fn inspect_exec(
    &self,
    id: &str,
  ) -> Result<ExecInspectResponse, HttpClientError> {
    let res = self
      .send_get(
        format!("/{}/exec/{id}/cargo/inspect", &self.version),
        Some(()),
      )
      .await?;

    Ok(Self::res_json(res).await.unwrap())
  }

  /// ## Run an command inside a cargo
  ///
  /// ## Arguments
  ///
  /// * [id](str) - Id of command to run
  /// * [exec](CreateExecOptions) - The config for the exec command
  /// * [namespace](Option<String>) - The namespace where belong the cargo
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///  * [Ok](Ok) - A [mpsc::Receiver](mpsc::Receiver) of [ExecOutput](ExecOutput)
  /// * [Err](HttpClientError) - The command could not be executed
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
  ///
  /// let result = client.create_exec("my-cargo", exec, None).await.unwrap();
  ///
  /// let mut rx = client.start_exec(&result.id, StartExec::default(), None).await.unwrap();
  /// while let Some(output) = rx.next().await {
  ///  println!("{}", output);
  /// };
  /// ```
  ///
  pub async fn start_exec(
    &self,
    id: &str,
    exec: StartExecOptions,
  ) -> Result<mpsc::Receiver<Result<OutputLog, HttpError>>, HttpClientError> {
    let res = self
      .send_post(
        format!("/{}/exec/{id}/cargo/start", &self.version),
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
    let client = NanocldClient::connect_to("http://localhost:8585", None);

    let exec = CreateExecOptions {
      cmd: Some(vec!["echo".into(), "hello".into()]),
      ..Default::default()
    };

    let result = client
      .create_exec("nstore", exec, Some("system".into()))
      .await
      .unwrap();

    let mut rx = client
      .start_exec(&result.id, StartExecOptions::default())
      .await
      .unwrap();
    while let Some(_out) = rx.next().await {}

    client.inspect_exec(&result.id).await.unwrap();
  }
}
