use ntex::channel::mpsc;
use ntex::channel::mpsc::Receiver;

use nanocl_utils::http_error::HttpError;
use nanocl_utils::http_client_error::HttpClientError;

use bollard_next::service::ContainerSummary;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::cargo::{
  Cargo, CargoSummary, CargoInspect, CreateExecOptions, OutputLog,
  CargoKillOptions, CargoDeleteQuery, CargoLogQuery,
};
use nanocl_stubs::cargo_config::{
  CargoConfigUpdate, CargoConfigPartial, CargoConfig,
};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Create a new cargo
  ///
  /// ## Arguments
  /// * [item](CargoConfigPartial) - The cargo config to create
  /// * [namespace](Option<String>) - The namespace to create the cargo in
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The created [cargo](Cargo)
  ///   * [Err](HttpClientError) - The cargo could not be created
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let new_cargo = CargoConfigPartial {
  ///  name: String::from("my-cargo"),
  ///  container: bollard_next::container::Config {
  ///    image: Some(String::from("alpine"))
  ///    ..Default::default()
  ///   }
  /// };
  /// let cargo = client.create_cargo(new_cargo, None).await;
  /// ```
  ///
  pub async fn create_cargo(
    &self,
    item: &CargoConfigPartial,
    namespace: Option<String>,
  ) -> Result<Cargo, HttpClientError> {
    let res = self
      .send_post(
        format!("/{}/cargoes", &self.version),
        Some(item),
        Some(&GenericNspQuery { namespace }),
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Delete a cargo
  /// Delete a cargo by it's name
  ///
  /// ## Arguments
  /// * [name](str) - The name of the cargo to delete
  /// * [namespace](Option<String>) - The namespace to delete the cargo from
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The cargo was deleted
  ///   * [Err](HttpClientError) - The cargo could not be deleted
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// client.delete_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn delete_cargo(
    &self,
    name: &str,
    query: &CargoDeleteQuery,
  ) -> Result<(), HttpClientError> {
    self
      .send_delete(format!("/{}/cargoes/{name}", &self.version), Some(query))
      .await?;

    Ok(())
  }

  /// ## Inspect a cargo
  /// Inspect a cargo by it's name to get more information about it
  ///
  /// ## Arguments
  /// * [name](str) - The name of the cargo to inspect
  /// * [namespace](Option<String>) - The namespace to inspect the cargo from
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The desired [cargo](Cargo)
  ///   * [Err](HttpClientError) - The cargo could not be inspected
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let cargo = client.inspect_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn inspect_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<CargoInspect, HttpClientError> {
    let res = self
      .send_get(
        format!("/{}/cargoes/{name}/inspect", &self.version),
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Self::res_json(res).await
  }

  /// ## Start a cargo
  /// Start a cargo by it's name
  ///
  /// ## Arguments
  /// * [name](str) - The name of the cargo to start
  /// * [namespace](Option<String>) - The namespace to start the cargo from
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The cargo was started
  ///   * [Err](HttpClientError) - The cargo could not be started
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// client.start_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn start_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), HttpClientError> {
    self
      .send_post(
        format!("/{}/cargoes/{name}/start", &self.version),
        None::<String>,
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Ok(())
  }

  /// # Stop a cargo
  /// Stop a cargo by it's name
  ///
  /// ## Arguments
  /// * [name](str) - The name of the cargo to stop
  /// * [namespace](Option<String>) - The namespace to stop the cargo from
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The cargo was stopped
  ///   * [Err](HttpClientError) - The cargo could not be stopped
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// client.stop_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn stop_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), HttpClientError> {
    self
      .send_post(
        format!("/{}/cargoes/{name}/stop", &self.version),
        None::<String>,
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Ok(())
  }

  /// ## List cargoes
  /// List all cargoes in a namespace
  ///
  /// ## Arguments
  /// * [namespace](Option<String>) - The namespace to list the cargoes from
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - A [Vec](Vec) of [cargoes](CargoSummary)
  ///   * [Err](HttpClientError) - The cargoes could not be listed
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let cargoes = client.list_cargoes(None).await.unwrap();
  /// ```
  ///
  pub async fn list_cargo(
    &self,
    namespace: Option<String>,
  ) -> Result<Vec<CargoSummary>, HttpClientError> {
    let res = self
      .send_get(
        format!("/{}/cargoes", &self.version),
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Self::res_json(res).await
  }

  /// ## Patch a cargo
  /// Patch a cargo by it's name
  /// This will update the cargo's config by merging current config with new config and creating an history entry
  ///
  /// ## Arguments
  /// * [name](str) - The name of the cargo to patch
  /// * [cargo](CargoConfigPatch) - The config to patch the cargo with
  /// * [namespace](Option<String>) - The namespace to patch the cargo from
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The cargo was patched
  ///   * [Err](HttpClientError) - The cargo could not be patched
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let cargo_config = CargoConfigPatch {
  ///   name: "my-cargo-renamed".into(),
  /// };
  /// client.patch_cargo("my-cargo", cargo, None).await.unwrap();
  /// ```
  ///
  pub async fn patch_cargo(
    &self,
    name: &str,
    config: CargoConfigUpdate,
    namespace: Option<String>,
  ) -> Result<(), HttpClientError> {
    self
      .send_patch(
        format!("/{}/cargoes/{name}", &self.version),
        Some(config),
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Ok(())
  }

  /// ## Put a cargo
  /// Put a cargo by it's name
  /// It will create a new cargo config and store old one in history
  ///
  /// ## Arguments
  /// * [name](str) - The name of the cargo to put
  /// * [cargo](CargoConfigPatch) - The config to put the cargo with
  /// * [namespace](Option<String>) - The namespace to put the cargo from
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The cargo was put
  ///   * [Err](HttpClientError) - The cargo could not be put
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let cargo_config = CargoConfigPatch {
  ///   name: "my-cargo-renamed".into(),
  /// };
  /// client.put_cargo("my-cargo", cargo, None).await.unwrap();
  /// ```
  ///
  pub async fn put_cargo(
    &self,
    name: &str,
    config: CargoConfigUpdate,
    namespace: Option<String>,
  ) -> Result<(), HttpClientError> {
    self
      .send_put(
        format!("/{}/cargoes/{name}", &self.version),
        Some(config),
        Some(GenericNspQuery { namespace }),
      )
      .await?;
    Ok(())
  }

  /// ## Exec command inside a cargo
  ///
  /// ## Arguments
  ///
  /// - [name](str) - The name of the cargo to exec the command in
  /// - [exec](CreateExecOptions) - The config for the exec command
  /// - [namespace](Option<String>) - The namespace where belong the cargo
  ///
  /// ## Returns
  ///
  /// - [Result](Result)
  ///  - [Ok](Ok) - A [mpsc::Receiver](mpsc::Receiver) of [ExecOutput](ExecOutput)
  /// - [Err](HttpClientError) - The command could not be executed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use futures::StreamExt;
  /// use nanocld_client::NanocldClient;
  /// use nanocld_client::models::cargo_config::CreateExecOptions;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let exec = CreateExecOptions {
  ///  cmd: vec!["echo".into(), "hello".into()],
  /// ..Default::default()
  /// };
  /// let mut rx = client.exec_cargo("my-cargo", exec, None).await.unwrap();
  /// while let Some(output) = rx.next().await {
  ///  println!("{}", output);
  /// };
  /// ```
  ///
  pub async fn exec_cargo(
    &self,
    name: &str,
    exec: CreateExecOptions,
    namespace: Option<String>,
  ) -> Result<mpsc::Receiver<Result<OutputLog, HttpError>>, HttpClientError> {
    let res = self
      .send_post(
        format!("/{}/cargoes/{name}/exec", &self.version),
        Some(exec),
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Ok(Self::res_stream(res).await)
  }

  /// ## List all the cargo histories
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to list the histories
  /// * [namespace](Option<String>) - The namespace where belong the cargo
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - A [Vec](Vec) of [CargoConfig](CargoConfig)
  ///   * [Err](HttpClientError) - The cargo could not be listed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let histories = client.list_history("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn list_history_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<Vec<CargoConfig>, HttpClientError> {
    let res = self
      .send_get(
        format!("/{}/cargoes/{name}/histories", &self.version),
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Self::res_json(res).await
  }

  /// ## Reset a cargo to a specific history
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to reset
  /// * [id](str) - The id of the history to reset to
  /// * [namespace](Option<String>) - The namespace where belong the cargo
  ///
  /// ## Returns
  ///
  /// * [Result](Result)
  ///   * [Ok](Ok) - The [Cargo](Cargo) reseted
  ///   * [Err](HttpClientError) - The cargo could not be reseted
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_with_unix_default();
  /// let cargo = client.reset_cargo("my-cargo", "my-history-id", None).await.unwrap();
  /// ```
  ///
  pub async fn reset_cargo(
    &self,
    name: &str,
    id: &str,
    namespace: Option<String>,
  ) -> Result<Cargo, HttpClientError> {
    let res = self
      .send_patch(
        format!("/{}/cargoes/{name}/histories/{id}/reset", &self.version),
        None::<String>,
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Self::res_json(res).await
  }

  /// ## Get the logs of a cargo
  /// The logs are streamed as a [Receiver](Receiver) of [CargoOutput](CargoOutput)
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to get the logs
  /// * [namespace](Option<String>) - The namespace where belong the cargo
  ///
  pub async fn logs_cargo(
    &self,
    name: &str,
    query: &CargoLogQuery,
  ) -> Result<Receiver<Result<OutputLog, HttpError>>, HttpClientError> {
    let res = self
      .send_get(
        format!("/{}/cargoes/{name}/logs", &self.version),
        Some(query),
      )
      .await?;

    Ok(Self::res_stream(res).await)
  }

  pub async fn kill_cargo(
    &self,
    name: &str,
    options: &CargoKillOptions,
    namespace: Option<String>,
  ) -> Result<(), HttpClientError> {
    self
      .send_post(
        format!("/{}/cargoes/{name}/kill", &self.version),
        Some(options),
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Ok(())
  }

  pub async fn list_cargo_instance(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<Vec<ContainerSummary>, HttpClientError> {
    let res = self
      .send_get(
        format!("/{}/cargoes/{name}/instances", &self.version),
        Some(GenericNspQuery { namespace }),
      )
      .await?;

    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use futures::StreamExt;
  use nanocl_stubs::cargo_config::CargoConfigPartial;
  use ntex::http;

  #[ntex::test]
  async fn basic() {
    const CARGO_NAME: &str = "client-test-cargo";
    let client = NanocldClient::connect_with_unix_default();

    client.list_cargo(None).await.unwrap();

    let new_cargo = CargoConfigPartial {
      name: CARGO_NAME.into(),
      container: bollard_next::container::Config {
        image: Some("nexthat/nanocl-get-started:latest".into()),
        ..Default::default()
      },
      ..Default::default()
    };
    client.create_cargo(&new_cargo, None).await.unwrap();

    client.start_cargo(CARGO_NAME, None).await.unwrap();
    client.inspect_cargo(CARGO_NAME, None).await.unwrap();

    let new_cargo = CargoConfigUpdate {
      container: Some(bollard_next::container::Config {
        image: Some("nexthat/nanocl-get-started:latest".into()),
        env: Some(vec!["TEST=1".into()]),
        ..Default::default()
      }),
      ..Default::default()
    };

    client
      .patch_cargo(CARGO_NAME, new_cargo.clone(), None)
      .await
      .unwrap();

    client.put_cargo(CARGO_NAME, new_cargo, None).await.unwrap();

    let histories = client.list_history_cargo(CARGO_NAME, None).await.unwrap();
    assert!(histories.len() > 1);

    let history = histories.first().unwrap();
    client
      .reset_cargo(CARGO_NAME, &history.key.to_string(), None)
      .await
      .unwrap();

    client.stop_cargo(CARGO_NAME, None).await.unwrap();
    client
      .delete_cargo(CARGO_NAME, &CargoDeleteQuery::default())
      .await
      .unwrap();
  }

  #[ntex::test]
  async fn create_cargo_wrong_image() {
    let client = NanocldClient::connect_with_unix_default();

    let new_cargo = CargoConfigPartial {
      name: "client-test-cargowi".into(),
      container: bollard_next::container::Config {
        image: Some("random_image:ggwp".into()),
        ..Default::default()
      },
      ..Default::default()
    };
    let err = client.create_cargo(&new_cargo, None).await.unwrap_err();
    match err {
      HttpClientError::HttpError(err) => {
        assert_eq!(err.status, http::StatusCode::NOT_FOUND);
      }
      _ => panic!("Wrong error type"),
    }
  }

  #[ntex::test]
  async fn create_cargo_duplicate_name() {
    let client = NanocldClient::connect_with_unix_default();

    let new_cargo = CargoConfigPartial {
      name: "client-test-cargodup".into(),
      container: bollard_next::container::Config {
        image: Some("nexthat/nanocl-get-started:latest".into()),
        ..Default::default()
      },
      ..Default::default()
    };
    client.create_cargo(&new_cargo, None).await.unwrap();

    let err = client.create_cargo(&new_cargo, None).await.unwrap_err();
    match err {
      HttpClientError::HttpError(err) => {
        assert_eq!(err.status, 409);
      }
      _ => panic!("Wrong error type"),
    }
    client
      .delete_cargo("client-test-cargodup", &CargoDeleteQuery::default())
      .await
      .unwrap();
  }

  #[ntex::test]
  async fn exec_cargo() {
    let client = NanocldClient::connect_with_unix_default();

    let exec = CreateExecOptions {
      cmd: Some(vec!["echo".into(), "hello".into()]),
      ..Default::default()
    };
    let mut rx = client
      .exec_cargo("nstore", exec, Some("system".into()))
      .await
      .unwrap();
    while let Some(_out) = rx.next().await {}
  }

  #[ntex::test]
  async fn logs_cargo() {
    let client = NanocldClient::connect_with_unix_default();

    let mut rx = client
      .logs_cargo("nstore", &CargoLogQuery::of_namespace("system".into()))
      .await
      .unwrap();
    let _out = rx.next().await.unwrap().unwrap();
  }
}
