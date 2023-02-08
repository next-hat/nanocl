use ntex::channel::mpsc;
use ntex::channel::mpsc::Receiver;

use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::cargo::{
  Cargo, CargoSummary, CargoInspect, CargoExecConfig, CargoOutput,
};
use nanocl_stubs::cargo_config::{
  CargoConfigPatch, CargoConfigPartial, CargoConfig,
};

use crate::error::ApiError;

use super::http_client::NanoclClient;
use super::error::{NanoclClientError, is_api_error};

impl NanoclClient {
  /// ## Create a new cargo
  ///
  /// ## Arguments
  /// * [item](CargoConfigPartial) - The cargo config to create
  /// * [namespace](Option<String>) - The namespace to create the cargo in
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The created [cargo](Cargo)
  ///   * [Err](NanoclClientError) - The cargo could not be created
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
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
  ) -> Result<Cargo, NanoclClientError> {
    let mut res = self
      .post(String::from("/cargoes"))
      .query(&GenericNspQuery { namespace })?
      .send_json(item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<Cargo>().await?;

    Ok(item)
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
  ///   * [Err](NanoclClientError) - The cargo could not be deleted
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// client.delete_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn delete_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .delete(format!("/cargoes/{name}"))
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

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
  ///   * [Err](NanoclClientError) - The cargo could not be inspected
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let cargo = client.inspect_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn inspect_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<CargoInspect, NanoclClientError> {
    let mut res = self
      .get(format!("/cargoes/{name}/inspect"))
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<CargoInspect>().await?;

    Ok(item)
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
  ///   * [Err](NanoclClientError) - The cargo could not be started
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// client.start_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn start_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .post(format!("/cargoes/{name}/start"))
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

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
  ///   * [Err](NanoclClientError) - The cargo could not be stopped
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// client.stop_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn stop_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .post(format!("/cargoes/{name}/stop"))
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

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
  ///   * [Err](NanoclClientError) - The cargoes could not be listed
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let cargoes = client.list_cargoes(None).await.unwrap();
  /// ```
  ///
  pub async fn list_cargo(
    &self,
    namespace: Option<String>,
  ) -> Result<Vec<CargoSummary>, NanoclClientError> {
    let mut res = self
      .get("/cargoes".into())
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<CargoSummary>>().await?;

    Ok(items)
  }

  /// ## Patch a cargo
  /// Patch a cargo by it's name
  /// This will update the cargo's config
  ///
  /// ## Arguments
  /// * [name](str) - The name of the cargo to patch
  /// * [cargo](CargoConfigPatch) - The config to patch the cargo with
  /// * [namespace](Option<String>) - The namespace to patch the cargo from
  ///
  /// ## Returns
  /// * [Result](Result)
  ///   * [Ok](Ok) - The cargo was patched
  ///   * [Err](NanoclClientError) - The cargo could not be patched
  ///
  /// ## Example
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let cargo_config = CargoConfigPatch {
  ///   name: "my-cargo-renamed".into(),
  /// };
  /// client.patch_cargo("my-cargo", cargo, None).await.unwrap();
  /// ```
  ///
  pub async fn patch_cargo(
    &self,
    name: &str,
    config: CargoConfigPatch,
    namespace: Option<String>,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .patch(format!("/cargoes/{name}"))
      .query(&GenericNspQuery { namespace })?
      .send_json(&config)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  /// ## Exec command inside a cargo
  ///
  /// ## Arguments
  ///
  /// - [name](str) - The name of the cargo to exec the command in
  /// - [exec](CargoExecConfig) - The config for the exec command
  /// - [namespace](Option<String>) - The namespace where belong the cargo
  ///
  /// ## Returns
  ///
  /// - [Result](Result)
  ///  - [Ok](Ok) - A [mpsc::Receiver](mpsc::Receiver) of [ExecOutput](ExecOutput)
  /// - [Err](NanoclClientError) - The command could not be executed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use futures::StreamExt;
  /// use nanocld_client::NanoclClient;
  /// use nanocld_client::models::cargo_config::CargoExecConfig;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let exec = CargoExecConfig {
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
    exec: CargoExecConfig<String>,
    namespace: Option<String>,
  ) -> Result<mpsc::Receiver<Result<CargoOutput, ApiError>>, NanoclClientError>
  {
    let mut res = self
      .post(format!("/cargoes/{name}/exec"))
      .query(&GenericNspQuery { namespace })?
      .send_json(&exec)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let rx = self.stream(res).await;

    Ok(rx)
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
  ///   * [Err](NanoclClientError) - The cargo could not be listed
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let histories = client.list_history("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn list_history_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<Vec<CargoConfig>, NanoclClientError> {
    let histories = self
      .get(format!("/cargoes/{name}/histories"))
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?
      .json::<Vec<CargoConfig>>()
      .await?;
    Ok(histories)
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
  ///   * [Err](NanoclClientError) - The cargo could not be reseted
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default();
  /// let cargo = client.reset_cargo("my-cargo", "my-history-id", None).await.unwrap();
  /// ```
  ///
  pub async fn reset_cargo(
    &self,
    name: &str,
    id: &str,
    namespace: Option<String>,
  ) -> Result<Cargo, NanoclClientError> {
    let mut res = self
      .patch(format!("/cargoes/{name}/histories/{id}/reset"))
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let cargo = res.json::<Cargo>().await?;
    Ok(cargo)
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
    namespace: Option<String>,
  ) -> Result<Receiver<Result<CargoOutput, ApiError>>, NanoclClientError> {
    let mut res = self
      .get(format!("/cargoes/{name}/logs"))
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let rx = self.stream(res).await;

    Ok(rx)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use futures::StreamExt;
  use nanocl_stubs::cargo_config::CargoConfigPartial;

  #[ntex::test]
  async fn test_basic() {
    const CARGO_NAME: &str = "client-test-cargo";
    let client = NanoclClient::connect_with_unix_default();

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

    let new_cargo = CargoConfigPatch {
      container: Some(bollard_next::container::Config {
        image: Some("nexthat/nanocl-get-started:latest".into()),
        env: Some(vec!["TEST=1".into()]),
        ..Default::default()
      }),
      ..Default::default()
    };

    client
      .patch_cargo(CARGO_NAME, new_cargo, None)
      .await
      .unwrap();

    let histories = client.list_history_cargo(CARGO_NAME, None).await.unwrap();
    assert!(histories.len() > 1);

    let history = histories.first().unwrap();
    client
      .reset_cargo(CARGO_NAME, &history.key.to_string(), None)
      .await
      .unwrap();

    client.stop_cargo(CARGO_NAME, None).await.unwrap();
    client.delete_cargo(CARGO_NAME, None).await.unwrap();
  }

  #[ntex::test]
  async fn test_create_cargo_wrong_image() {
    let client = NanoclClient::connect_with_unix_default();

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
      NanoclClientError::Api(err) => {
        assert_eq!(err.status, 400);
      }
      _ => panic!("Wrong error type"),
    }
  }

  #[ntex::test]
  async fn test_create_cargo_duplicate_name() {
    let client = NanoclClient::connect_with_unix_default();

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
      NanoclClientError::Api(err) => {
        assert_eq!(err.status, 409);
      }
      _ => panic!("Wrong error type"),
    }
    client
      .delete_cargo("client-test-cargodup", None)
      .await
      .unwrap();
  }

  #[ntex::test]
  async fn test_exec_cargo() {
    let client = NanoclClient::connect_with_unix_default();

    let exec = CargoExecConfig {
      cmd: Some(vec!["echo".into(), "hello".into()]),
      ..Default::default()
    };
    let mut rx = client
      .exec_cargo("store", exec, Some("system".into()))
      .await
      .unwrap();
    while let Some(_out) = rx.next().await {}
  }

  #[ntex::test]
  async fn test_logs_cargo() {
    let client = NanoclClient::connect_with_unix_default();

    let mut rx = client
      .logs_cargo("store", Some("system".into()))
      .await
      .unwrap();
    let _out = rx.next().await.unwrap().unwrap();
  }
}
