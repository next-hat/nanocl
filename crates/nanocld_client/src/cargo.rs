use bollard_next::service::ContainerSummary;

use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::{
  cargo::{Cargo, CargoDeleteQuery, CargoInspect, CargoSummary},
  cargo_spec::{CargoSpec, CargoSpecPartial, CargoSpecUpdate},
  generic::{GenericFilterNsp, GenericNspQuery},
};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for cargoes
  const CARGO_PATH: &'static str = "/cargoes";

  /// Create a new cargo in the system
  /// Note that the cargo is not started by default
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let new_cargo = CargoSpecPartial {
  ///  name: String::from("my-cargo"),
  ///  container: bollard_next::container::Config {
  ///    image: Some(String::from("alpine"))
  ///    ..Default::default()
  ///   }
  /// };
  /// let res = client.create_cargo(new_cargo, None).await;
  /// ```
  pub async fn create_cargo(
    &self,
    item: &CargoSpecPartial,
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

  /// Delete a cargo by its canonical key.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.delete_cargo("global.my-cargo", None).await;
  /// ```
  pub async fn delete_cargo(
    &self,
    key: &str,
    query: Option<&CargoDeleteQuery>,
  ) -> HttpClientResult<()> {
    self
      .send_delete(&format!("{}/{key}", Self::CARGO_PATH), query)
      .await?;
    Ok(())
  }

  /// Inspect a cargo by its canonical key.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.inspect_cargo("global.my-cargo").await;
  /// ```
  pub async fn inspect_cargo(
    &self,
    key: &str,
  ) -> HttpClientResult<CargoInspect> {
    let res = self
      .send_get(
        &format!("{}/{key}/inspect", Self::CARGO_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// List cargoes, optionally filtering by namespace.
  ///
  /// An omitted, empty, or whitespace-only namespace returns cargoes from all
  /// namespaces.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let cargoes = client.list_cargoes(None).await.unwrap();
  /// ```
  pub async fn list_cargo(
    &self,
    query: Option<&GenericFilterNsp>,
  ) -> HttpClientResult<Vec<CargoSummary>> {
    let query = Self::convert_query(query)?;
    let res = self.send_get(Self::CARGO_PATH, Some(query)).await?;
    Self::res_json(res).await
  }

  /// Patch a cargo by its canonical key.
  /// This will update the cargo's spec by merging current spec with new spec and creating an history entry
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let cargo_spec = CargoSpecUpdate {
  ///   name: Some("my-cargo".into()),
  ///   ..Default::default()
  /// };
  /// client.patch_cargo("global.my-cargo", &cargo_spec).await.unwrap();
  /// ```
  pub async fn patch_cargo(
    &self,
    key: &str,
    spec: &CargoSpecUpdate,
  ) -> HttpClientResult<()> {
    self
      .send_patch(
        &format!("{}/{key}", Self::CARGO_PATH),
        Some(spec),
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// Put a cargo by its canonical key.
  /// It will create a new cargo spec and store old one in history
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let cargo_spec = CargoSpecPartial {
  ///   name: "my-cargo".into(),
  ///   ..Default::default()
  /// };
  /// client.put_cargo("global.my-cargo", &cargo_spec).await.unwrap();
  /// ```
  pub async fn put_cargo(
    &self,
    key: &str,
    spec: &CargoSpecPartial,
  ) -> HttpClientResult<()> {
    self
      .send_put(
        &format!("{}/{key}", Self::CARGO_PATH),
        Some(spec),
        None::<String>,
      )
      .await?;
    Ok(())
  }

  /// List cargo histories
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let histories = client
  ///   .list_history_cargo("global.my-cargo")
  ///   .await
  ///   .unwrap();
  /// ```
  pub async fn list_history_cargo(
    &self,
    key: &str,
  ) -> HttpClientResult<Vec<CargoSpec>> {
    let res = self
      .send_get(
        &format!("{}/{key}/histories", Self::CARGO_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// Revert a cargo to a specific history
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let cargo = client.revert_cargo("global.my-cargo", "my-history-id").await.unwrap();
  /// ```
  pub async fn revert_cargo(
    &self,
    key: &str,
    id: &str,
  ) -> HttpClientResult<Cargo> {
    let res = self
      .send_patch(
        &format!("{}/{key}/histories/{id}/revert", Self::CARGO_PATH),
        None::<String>,
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }

  /// List all instances of a cargo by its canonical key.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_cargo_instance("global.my-cargo").await;
  /// ```
  pub async fn list_cargo_instance(
    &self,
    key: &str,
  ) -> HttpClientResult<Vec<ContainerSummary>> {
    let res = self
      .send_get(
        &format!("{}/{key}/instances", Self::CARGO_PATH),
        None::<String>,
      )
      .await?;
    Self::res_json(res).await
  }
}

#[cfg(test)]
mod tests {
  use crate::ConnectOpts;

  use super::*;

  use nanocl_error::http_client::HttpClientError;
  use nanocl_stubs::cargo_spec::CargoSpecPartial;

  #[ntex::test]
  async fn basic() {
    const CARGO_NAME: &str = "client-test-cargo";
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    client.list_cargo(None).await.unwrap();
    let new_cargo = CargoSpecPartial {
      name: CARGO_NAME.into(),
      container: bollard_next::container::Config {
        image: Some("ghcr.io/next-hat/nanocl-get-started:latest".into()),
        ..Default::default()
      },
      ..Default::default()
    };
    client.create_cargo(&new_cargo, None).await.unwrap();
    let cargo_key = format!("global.{CARGO_NAME}");
    client.start_process("cargo", &cargo_key).await.unwrap();
    client.inspect_cargo(&cargo_key).await.unwrap();
    let cargo_update = CargoSpecUpdate {
      container: Some(bollard_next::container::Config {
        image: Some("ghcr.io/next-hat/nanocl-get-started:latest".into()),
        env: Some(vec!["TEST=1".into()]),
        ..Default::default()
      }),
      ..Default::default()
    };
    client.patch_cargo(&cargo_key, &cargo_update).await.unwrap();
    client.put_cargo(&cargo_key, &new_cargo).await.unwrap();
    let histories = client.list_history_cargo(&cargo_key).await.unwrap();
    assert!(histories.len() > 1);
    let history = histories.first().unwrap();
    client
      .revert_cargo(&cargo_key, &history.key.to_string())
      .await
      .unwrap();
    client.stop_process("cargo", &cargo_key).await.unwrap();
    client
      .delete_cargo(&cargo_key, Some(&CargoDeleteQuery { force: Some(true) }))
      .await
      .unwrap();
  }

  #[ntex::test]
  async fn create_cargo_duplicate_name() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    let new_cargo = CargoSpecPartial {
      name: "client-test-cargodup".into(),
      container: bollard_next::container::Config {
        image: Some("ghcr.io/next-hat/nanocl-get-started:latest".into()),
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
      .delete_cargo("global.client-test-cargodup", None)
      .await
      .unwrap();
  }
}
