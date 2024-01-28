use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpResult;
use nanocl_error::http_client::HttpClientResult;

use bollard_next::service::ExecInspectResponse;
use bollard_next::exec::{CreateExecResults, StartExecOptions};
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::cargo::CreateExecOptions;
use nanocl_stubs::process::OutputLog;

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for exec commands
  const EXEC_PATH: &'static str = "/exec";

  /// Create exec command inside a cargo
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
  pub async fn create_exec(
    &self,
    name: &str,
    exec: &CreateExecOptions,
    namespace: Option<&str>,
  ) -> HttpClientResult<CreateExecResults> {
    let res = self
      .send_post(
        &format!("/cargoes/{name}/exec"),
        Some(exec),
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Self::res_json(res).await
  }

  /// Inspect an exec command inside a cargo instance.
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
  pub async fn inspect_exec(
    &self,
    id: &str,
  ) -> HttpClientResult<ExecInspectResponse> {
    let res = self
      .send_get(&format!("{}/{id}/cargo/inspect", Self::EXEC_PATH), Some(()))
      .await?;
    Self::res_json(res).await
  }

  /// Run an command inside a cargo
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
  pub async fn start_exec(
    &self,
    id: &str,
    exec: &StartExecOptions,
  ) -> HttpClientResult<Receiver<HttpResult<OutputLog>>> {
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

  use crate::{ConnectOpts, NanocldClient};

  #[ntex::test]
  async fn exec_cargo() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    });
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
