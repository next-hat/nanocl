use ntex::channel::mpsc::Receiver;

use nanocl_error::http::HttpResult;
use nanocl_error::http_client::HttpClientResult;

use bollard_next::service::ContainerSummary;
use nanocl_stubs::generic::GenericNspQuery;
use nanocl_stubs::cargo::{
  Cargo, CargoSummary, CargoInspect, OutputLog, CargoKillOptions,
  CargoDeleteQuery, CargoLogQuery, CargoStatsQuery, CargoStats,
};
use nanocl_stubs::cargo_config::{
  CargoConfigUpdate, CargoConfigPartial, CargoConfig,
};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for cargoes
  const CARGO_PATH: &'static str = "/cargoes";

  /// ## Create cargo
  ///
  /// Create a new cargo in the system
  /// Note that the cargo is not started by default
  ///
  /// ## Arguments
  ///
  /// * [item](CargoConfigPartial) - A reference of a [cargo config partial](CargoConfigPartial)
  /// * [namespace](Option) - The [namespace](str) to create the cargo in
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing the [cargo](Cargo) created
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let new_cargo = CargoConfigPartial {
  ///  name: String::from("my-cargo"),
  ///  container: bollard_next::container::Config {
  ///    image: Some(String::from("alpine"))
  ///    ..Default::default()
  ///   }
  /// };
  /// let res = client.create_cargo(new_cargo, None).await;
  /// ```
  ///
  pub async fn create_cargo(
    &self,
    item: &CargoConfigPartial,
    namespace: Option<&str>,
  ) -> HttpClientResult<Cargo> {
    let res = self
      .send_post(
        Self::CARGO_PATH,
        Some(item),
        Some(&GenericNspQuery::new(namespace)),
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Delete a cargo
  ///
  /// Delete a cargo by it's name and namespace
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to delete
  /// * [query](CargoDeleteQuery) - The [namespace](str) where the cargo belongs
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_cargo("my-cargo", None).await;
  /// ```
  ///
  pub async fn delete_cargo(
    &self,
    name: &str,
    query: Option<&CargoDeleteQuery>,
  ) -> HttpClientResult<()> {
    self
      .send_delete(&format!("{}/{name}", Self::CARGO_PATH), query)
      .await?;
    Ok(())
  }

  /// ## Inspect a cargo
  ///
  /// Inspect a cargo by it's name to get more information about it
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to inspect
  /// * [namespace](Option) - The [namespace](str) where the cargo belongs
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [cargo inspect](CargoInspect)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.inspect_cargo("my-cargo", None).await;
  /// ```
  ///
  pub async fn inspect_cargo(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<CargoInspect> {
    let res = self
      .send_get(
        &format!("{}/{name}/inspect", Self::CARGO_PATH),
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Start a cargo
  ///
  /// Start a cargo by it's name and namespace
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to start
  /// * [namespace](Option) - The [namespace](str) where the cargo belongs
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.start_cargo("my-cargo", None).await;
  /// ```
  ///
  pub async fn start_cargo(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{name}/start", Self::CARGO_PATH),
        None::<String>,
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// # Stop a cargo
  ///
  /// Stop a cargo by it's name and namespace
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to stop
  /// * [namespace](Option) - The [namespace](str) where the cargo belongs
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.stop_cargo("my-cargo", None).await;
  /// ```
  ///
  pub async fn stop_cargo(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{name}/stop", Self::CARGO_PATH),
        None::<String>,
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// # Restart a cargo
  ///
  /// Restart a cargo by it's name and namespace
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to restart
  /// * [namespace](Option) - The [namespace](str) where the cargo belongs
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.restart_cargo("my-cargo", None).await;
  /// ```
  ///
  pub async fn restart_cargo(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{name}/restart", Self::CARGO_PATH),
        None::<String>,
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// ## List cargoes
  ///
  /// List all cargoes in a namespace
  ///
  /// ## Arguments
  ///
  /// * [namespace](Option) - The [namespace](str) where the cargoes belongs
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Vec](Vec) of [CargoSummary](CargoSummary)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let cargoes = client.list_cargoes(None).await.unwrap();
  /// ```
  ///
  pub async fn list_cargo(
    &self,
    namespace: Option<&str>,
  ) -> HttpClientResult<Vec<CargoSummary>> {
    let res = self
      .send_get(Self::CARGO_PATH, Some(GenericNspQuery::new(namespace)))
      .await?;
    Self::res_json(res).await
  }

  /// ## Patch a cargo
  ///
  /// Patch a cargo by it's name
  /// This will update the cargo's config by merging current config with new config and creating an history entry
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to patch
  /// * [cargo](CargoConfigUpdate) - The config to patch the cargo with
  /// * [namespace](Option) - The [namespace](str) where the cargo belongs
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let cargo_config = CargoConfigPatch {
  ///   name: "my-cargo-renamed".into(),
  /// };
  /// client.patch_cargo("my-cargo", cargo, None).await.unwrap();
  /// ```
  ///
  pub async fn patch_cargo(
    &self,
    name: &str,
    config: &CargoConfigUpdate,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_patch(
        &format!("{}/{name}", Self::CARGO_PATH),
        Some(config),
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// ## Put a cargo
  ///
  /// Put a cargo by it's name
  /// It will create a new cargo config and store old one in history
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to update
  /// * [cargo](CargoConfigPartial) - The config to update the cargo with
  /// * [namespace](Option) - The [namespace](str) where the cargo belongs
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let cargo_config = CargoConfigPartial {
  ///   name: "my-cargo-renamed".into(),
  /// };
  /// client.put_cargo("my-cargo", &cargo, None).await.unwrap();
  /// ```
  ///
  pub async fn put_cargo(
    &self,
    name: &str,
    config: &CargoConfigPartial,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_put(
        &format!("{}/{name}", Self::CARGO_PATH),
        Some(config),
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// ## List all the cargo histories
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to list the histories
  /// * [namespace](Option) - The [namespace](str) where belong the cargo
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Vec](Vec) of [CargoConfig](CargoConfig)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let histories = client.list_history("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn list_history_cargo(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<Vec<CargoConfig>> {
    let res = self
      .send_get(
        &format!("{}/{name}/histories", Self::CARGO_PATH),
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Revert a cargo to a specific history
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to revert
  /// * [id](str) - The id of the history to revert to
  /// * [namespace](Option) - The [namespace](str) where belong the cargo
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing the [cargo](Cargo) reverted
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let cargo = client.revert_cargo("my-cargo", "my-history-id", None).await.unwrap();
  /// ```
  ///
  pub async fn revert_cargo(
    &self,
    name: &str,
    id: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<Cargo> {
    let res = self
      .send_patch(
        &format!("{}/{name}/histories/{id}/revert", Self::CARGO_PATH),
        None::<String>,
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Self::res_json(res).await
  }

  /// ## Logs a cargo
  ///
  /// Get logs of a cargo by it's name
  /// The logs are streamed as a [Receiver](Receiver) of [output log](OutputLog)
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to get the logs
  /// * [query](Option) - The optional [query](CargoLogQuery)
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Receiver](Receiver) of [output log](OutputLog)
  ///
  pub async fn logs_cargo(
    &self,
    name: &str,
    query: Option<&CargoLogQuery>,
  ) -> HttpClientResult<Receiver<HttpResult<OutputLog>>> {
    let res = self
      .send_get(&format!("{}/{name}/logs", Self::CARGO_PATH), query)
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Get the stats of a cargo
  ///
  /// The stats are streamed as a [Receiver](Receiver) of [cargo stats](CargoStats)
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to get the stats
  /// * [query](Option) - The option [query](CargoStatsQuery)
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Receiver](Receiver) of [cargo stats](CargoStats)
  ///
  pub async fn stats_cargo(
    &self,
    name: &str,
    query: Option<&CargoStatsQuery>,
  ) -> HttpClientResult<Receiver<HttpResult<CargoStats>>> {
    let res = self
      .send_get(&format!("{}/{name}/stats", Self::CARGO_PATH), query)
      .await?;
    Ok(Self::res_stream(res).await)
  }

  /// ## Kill a cargo
  ///
  /// Kill a cargo by it's name
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to kill
  /// * [query](Option) - The optional [query](CargoKillOptions)
  /// * [namespace](Option) - The [namespace](str) where belong the cargo
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.kill_cargo("my-cargo", None, None).await;
  /// ```
  ///
  pub async fn kill_cargo(
    &self,
    name: &str,
    query: Option<&CargoKillOptions>,
    namespace: Option<&str>,
  ) -> HttpClientResult<()> {
    self
      .send_post(
        &format!("{}/{name}/kill", Self::CARGO_PATH),
        query,
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Ok(())
  }

  /// ## List cargo instance
  ///
  /// List all the instances of a cargo by it's name and namespace
  ///
  /// ## Arguments
  ///
  /// * [name](str) - The name of the cargo to list the instances
  /// * [namespace](Option) - The [namespace](str) where belong the cargo
  ///
  /// ## Return
  ///
  /// [HttpClientResult](HttpClientResult) containing a [Vec](Vec) of [ContainerSummary](ContainerSummary)
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_cargo_instance("my-cargo", None).await;
  /// ```
  ///
  pub async fn list_cargo_instance(
    &self,
    name: &str,
    namespace: Option<&str>,
  ) -> HttpClientResult<Vec<ContainerSummary>> {
    let res = self
      .send_get(
        &format!("{}/{name}/instances", Self::CARGO_PATH),
        Some(GenericNspQuery::new(namespace)),
      )
      .await?;
    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use futures::StreamExt;
  use nanocl_error::http_client::HttpClientError;
  use nanocl_stubs::cargo_config::CargoConfigPartial;
  use ntex::http;

  #[ntex::test]
  async fn basic() {
    const CARGO_NAME: &str = "client-test-cargo";
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    client.list_cargo(None).await.unwrap();
    let new_cargo = CargoConfigPartial {
      name: CARGO_NAME.into(),
      container: bollard_next::container::Config {
        image: Some("ghcr.io/nxthat/nanocl-get-started:latest".into()),
        ..Default::default()
      },
      ..Default::default()
    };
    client.create_cargo(&new_cargo, None).await.unwrap();
    client.start_cargo(CARGO_NAME, None).await.unwrap();
    client.inspect_cargo(CARGO_NAME, None).await.unwrap();
    let cargo_update = CargoConfigUpdate {
      container: Some(bollard_next::container::Config {
        image: Some("ghcr.io/nxthat/nanocl-get-started:latest".into()),
        env: Some(vec!["TEST=1".into()]),
        ..Default::default()
      }),
      ..Default::default()
    };
    client
      .patch_cargo(CARGO_NAME, &cargo_update, None)
      .await
      .unwrap();
    client
      .put_cargo(CARGO_NAME, &new_cargo, None)
      .await
      .unwrap();
    let histories = client.list_history_cargo(CARGO_NAME, None).await.unwrap();
    assert!(histories.len() > 1);
    let history = histories.first().unwrap();
    client
      .revert_cargo(CARGO_NAME, &history.key.to_string(), None)
      .await
      .unwrap();
    client.stop_cargo(CARGO_NAME, None).await.unwrap();
    client.delete_cargo(CARGO_NAME, None).await.unwrap();
  }

  #[ntex::test]
  async fn create_cargo_wrong_image() {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
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
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let new_cargo = CargoConfigPartial {
      name: "client-test-cargodup".into(),
      container: bollard_next::container::Config {
        image: Some("ghcr.io/nxthat/nanocl-get-started:latest".into()),
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
      .delete_cargo("client-test-cargodup", None)
      .await
      .unwrap();
  }

  #[ntex::test]
  async fn logs_cargo() {
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let mut rx = client
      .logs_cargo("nstore", Some(&CargoLogQuery::of_namespace("system")))
      .await
      .unwrap();
    let _out = rx.next().await.unwrap().unwrap();
  }
}
