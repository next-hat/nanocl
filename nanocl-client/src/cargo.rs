use nanocl_models::generic::GenericNspQuery;
use nanocl_models::cargo::{Cargo, CargoSummary};
use nanocl_models::cargo_config::{CargoConfigPatch, CargoConfigPartial};

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
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let new_cargo = CargoConfigPartial {
  ///  name: String::from("my-cargo"),
  ///  container: bollard::container::Config {
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
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// client.delete_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn delete_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .delete(format!("/cargoes/{}", name))
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
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let cargo = client.inspect_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn inspect_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<Cargo, NanoclClientError> {
    let mut res = self
      .get(format!("/cargoes/{}/inspect", name))
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<Cargo>().await?;

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
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// client.start_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn start_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .post(format!("/cargoes/{}/start", name))
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
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// client.stop_cargo("my-cargo", None).await.unwrap();
  /// ```
  ///
  pub async fn stop_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .post(format!("/cargoes/{}/stop", name))
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
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
  /// let cargoes = client.list_cargoes(None).await.unwrap();
  /// ```
  ///
  pub async fn list_cargoes(
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
  /// ```rust,norun
  /// use nanocl_client::NanoclClient;
  ///
  /// let client = NanoclClient::connect_with_unix_default().await;
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
      .patch(format!("/cargoes/{}", name))
      .query(&GenericNspQuery { namespace })?
      .send_json(&config)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use nanocl_models::cargo_config::CargoConfigPartial;

  #[ntex::test]
  async fn test_basic() {
    const CARGO: &str = "client-test-cargo";
    let client = NanoclClient::connect_with_unix_default().await;

    client.list_cargoes(None).await.unwrap();

    let new_cargo = CargoConfigPartial {
      name: CARGO.into(),
      container: bollard::container::Config {
        image: Some("nexthat/nanocl-get-started:latest".into()),
        ..Default::default()
      },
      ..Default::default()
    };
    client.create_cargo(&new_cargo, None).await.unwrap();

    // let cargo = client.inspect_cargo(CARGO, None).await.unwrap();
    // assert_eq!(cargo.name, CARGO);

    client.start_cargo(CARGO, None).await.unwrap();

    let new_cargo = CargoConfigPatch {
      container: Some(bollard::container::Config {
        image: Some("nexthat/nanocl-get-started:latest".into()),
        env: Some(vec!["TEST=1".into()]),
        ..Default::default()
      }),
      ..Default::default()
    };

    client.patch_cargo(CARGO, new_cargo, None).await.unwrap();

    client.stop_cargo(CARGO, None).await.unwrap();
    client.delete_cargo(CARGO, None).await.unwrap();
  }

  #[ntex::test]
  async fn test_create_cargo_wrong_image() {
    let client = NanoclClient::connect_with_unix_default().await;

    let new_cargo = CargoConfigPartial {
      name: "client-test-cargowi".into(),
      container: bollard::container::Config {
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
    let client = NanoclClient::connect_with_unix_default().await;

    let new_cargo = CargoConfigPartial {
      name: "client-test-cargodup".into(),
      container: bollard::container::Config {
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
}
