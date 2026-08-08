use bollard_next::service::ContainerSummary;

use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::{
  cargo::{Cargo, CargoDeleteQuery, CargoInspect, CargoSummary},
  cargo_spec::{CargoSpec, CargoSpecPatch, CargoSpecRevision},
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
  /// use nanocld_client::{
  ///   ConnectOpts, NanocldClient, bollard_next,
  ///   stubs::{
  ///     cargo_spec::{CargoSpec, ContainerSpec},
  ///     generic::ImagePullPolicy,
  ///   },
  /// };
  ///
  /// let client = NanocldClient::connect_to(&ConnectOpts {
  ///   url: "http://localhost:8585".into(),
  ///   ..Default::default()
  /// }).unwrap();
  /// let new_cargo = CargoSpec {
  ///  name: String::from("my-cargo"),
  ///  containers: vec![ContainerSpec {
  ///    name: String::from("main"),
  ///    essential: true,
  ///    secrets: Vec::new(),
  ///    image_pull_secret: None,
  ///    image_pull_policy: ImagePullPolicy::IfNotPresent,
  ///    container_config: bollard_next::container::Config {
  ///      image: Some(String::from("alpine")),
  ///      ..Default::default()
  ///    },
  ///  }],
  ///  ..Default::default()
  /// };
  /// let res = client.create_cargo(&new_cargo, None).await;
  /// ```
  pub async fn create_cargo(
    &self,
    item: &CargoSpec,
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
  /// Fields present in the patch replace the corresponding complete desired
  /// fields, then the daemon creates a history entry.
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::{
  ///   ConnectOpts, NanocldClient,
  ///   stubs::cargo_spec::CargoSpecPatch,
  /// };
  ///
  /// let client = NanocldClient::connect_to(&ConnectOpts {
  ///   url: "http://localhost:8585".into(),
  ///   ..Default::default()
  /// }).unwrap();
  /// let cargo_spec = CargoSpecPatch {
  ///   replicas: Some(2),
  ///   ..Default::default()
  /// };
  /// client.patch_cargo("global.my-cargo", &cargo_spec).await.unwrap();
  /// ```
  pub async fn patch_cargo(
    &self,
    key: &str,
    spec: &CargoSpecPatch,
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
  /// use nanocld_client::{
  ///   ConnectOpts, NanocldClient, bollard_next,
  ///   stubs::{
  ///     cargo_spec::{CargoSpec, ContainerSpec},
  ///     generic::ImagePullPolicy,
  ///   },
  /// };
  ///
  /// let client = NanocldClient::connect_to(&ConnectOpts {
  ///   url: "http://localhost:8585".into(),
  ///   ..Default::default()
  /// }).unwrap();
  /// let cargo_spec = CargoSpec {
  ///   name: "my-cargo".into(),
  ///   containers: vec![ContainerSpec {
  ///     name: "main".into(),
  ///     essential: true,
  ///     secrets: Vec::new(),
  ///     image_pull_secret: None,
  ///     image_pull_policy: ImagePullPolicy::IfNotPresent,
  ///     container_config: bollard_next::container::Config {
  ///       image: Some("alpine".into()),
  ///       ..Default::default()
  ///     },
  ///   }],
  ///   ..Default::default()
  /// };
  /// client.put_cargo("global.my-cargo", &cargo_spec).await.unwrap();
  /// ```
  pub async fn put_cargo(
    &self,
    key: &str,
    spec: &CargoSpec,
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
  /// use nanocld_client::{
  ///   ConnectOpts, NanocldClient,
  ///   stubs::cargo_spec::CargoSpecRevision,
  /// };
  ///
  /// let client = NanocldClient::connect_to(&ConnectOpts {
  ///   url: "http://localhost:8585".into(),
  ///   ..Default::default()
  /// }).unwrap();
  /// let histories: Vec<CargoSpecRevision> = client
  ///   .list_history_cargo("global.my-cargo")
  ///   .await
  ///   .unwrap();
  /// ```
  pub async fn list_history_cargo(
    &self,
    key: &str,
  ) -> HttpClientResult<Vec<CargoSpecRevision>> {
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
  use std::sync::{Arc, Mutex};

  use ntex::web;
  use serde_json::{Value, json};

  use crate::ConnectOpts;

  use super::*;

  use nanocl_error::http_client::HttpClientError;
  use nanocl_stubs::{
    cargo_spec::{Config, ContainerSpec},
    generic::ImagePullPolicy,
  };

  fn container_spec(name: &str, image: &str) -> ContainerSpec {
    ContainerSpec {
      name: name.to_owned(),
      essential: true,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: ImagePullPolicy::IfNotPresent,
      container_config: Config {
        image: Some(image.to_owned()),
        ..Default::default()
      },
    }
  }

  fn cargo_spec(name: &str) -> CargoSpec {
    CargoSpec {
      name: name.to_owned(),
      containers: vec![container_spec(
        "main",
        "ghcr.io/next-hat/nanocl-get-started:latest",
      )],
      ..Default::default()
    }
  }

  #[derive(Clone, Default)]
  struct RequestRecorder(Arc<Mutex<Vec<(&'static str, Value)>>>);

  impl RequestRecorder {
    fn record(&self, method: &'static str, body: Value) {
      self.0.lock().unwrap().push((method, body));
    }

    fn body(&self, method: &'static str) -> Value {
      self
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|(recorded_method, _)| *recorded_method == method)
        .map(|(_, body)| body.clone())
        .unwrap_or_else(|| panic!("no {method} request was recorded"))
    }
  }

  fn revision_json(mut spec: Value) -> Value {
    let fields = spec
      .as_object_mut()
      .expect("CargoSpec must serialize as an object");
    fields.insert(
      "Key".to_owned(),
      json!("00000000-0000-0000-0000-000000000000"),
    );
    fields.insert("CargoKey".to_owned(), json!("global.client-wire"));
    fields.insert("Version".to_owned(), json!("v0.18.0"));
    fields.insert("CreatedAt".to_owned(), json!("1970-01-01T00:00:00"));
    spec
  }

  async fn record_create(
    recorder: web::types::State<RequestRecorder>,
    body: web::types::Json<Value>,
  ) -> web::HttpResponse {
    let body = body.into_inner();
    recorder.record("POST", body.clone());
    web::HttpResponse::Created().json(&json!({
      "NamespaceName": "global",
      "CreatedAt": "1970-01-01T00:00:00",
      "Status": {
        "UpdatedAt": "1970-01-01T00:00:00",
        "Wanted": "create",
        "PrevWanted": "create",
        "Actual": "create",
        "PrevActual": "create",
        "Health": "unknown"
      },
      "Spec": revision_json(body)
    }))
  }

  async fn record_put(
    recorder: web::types::State<RequestRecorder>,
    body: web::types::Json<Value>,
  ) -> web::HttpResponse {
    recorder.record("PUT", body.into_inner());
    web::HttpResponse::Ok().finish()
  }

  async fn record_patch(
    recorder: web::types::State<RequestRecorder>,
    body: web::types::Json<Value>,
  ) -> web::HttpResponse {
    recorder.record("PATCH", body.into_inner());
    web::HttpResponse::Ok().finish()
  }

  async fn serve_history() -> web::HttpResponse {
    let spec = serde_json::to_value(cargo_spec("client-wire")).unwrap();
    web::HttpResponse::Ok().json(&vec![revision_json(spec)])
  }

  async fn wire_test_client()
  -> (NanocldClient, RequestRecorder, web::test::TestServer) {
    let recorder = RequestRecorder::default();
    let server_recorder = recorder.clone();
    let server = web::test::server(async move || {
      web::App::new()
        .state(server_recorder.clone())
        .service(
          web::resource("/v0.18.0/cargoes")
            .route(web::post().to(record_create)),
        )
        .service(
          web::resource("/v0.18.0/cargoes/{key}")
            .route(web::put().to(record_put))
            .route(web::patch().to(record_patch)),
        )
        .service(
          web::resource("/v0.18.0/cargoes/{key}/histories")
            .route(web::get().to(serve_history)),
        )
    })
    .await;
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: format!("http://localhost:{}", server.addr().port()),
      ..Default::default()
    })
    .unwrap();
    (client, recorder, server)
  }

  #[ntex::test]
  async fn create_cargo_serializes_final_spec() {
    let (client, recorder, _server) = wire_test_client().await;
    let mut spec = cargo_spec("client-wire");
    spec.replicas = 2;
    spec.secrets = vec!["shared-secret".to_owned()];
    spec.init_containers = vec![container_spec("migrate", "alpine:latest")];

    let cargo = client.create_cargo(&spec, None).await.unwrap();

    assert_eq!(recorder.body("POST"), serde_json::to_value(&spec).unwrap());
    assert_eq!(cargo.spec.containers, spec.containers);
    assert_eq!(cargo.spec.init_containers, spec.init_containers);
  }

  #[ntex::test]
  async fn put_cargo_serializes_final_spec() {
    let (client, recorder, _server) = wire_test_client().await;
    let mut spec = cargo_spec("client-wire");
    spec.replicas = 3;

    client.put_cargo("global.client-wire", &spec).await.unwrap();

    assert_eq!(recorder.body("PUT"), serde_json::to_value(&spec).unwrap());
  }

  #[ntex::test]
  async fn patch_cargo_serializes_final_patch() {
    let (client, recorder, _server) = wire_test_client().await;
    let patch = CargoSpecPatch {
      replicas: Some(4),
      secrets: Some(vec!["replacement-secret".to_owned()]),
      containers: Some(vec![container_spec("worker", "busybox:latest")]),
      ..Default::default()
    };

    client
      .patch_cargo("global.client-wire", &patch)
      .await
      .unwrap();

    assert_eq!(
      recorder.body("PATCH"),
      serde_json::to_value(&patch).unwrap()
    );
  }

  #[ntex::test]
  async fn list_history_cargo_deserializes_revisions() {
    let (client, _recorder, _server) = wire_test_client().await;

    let histories = client
      .list_history_cargo("global.client-wire")
      .await
      .unwrap();

    assert_eq!(histories.len(), 1);
    assert_eq!(histories[0].cargo_key, "global.client-wire");
    assert_eq!(
      histories[0].containers,
      cargo_spec("client-wire").containers
    );
  }

  #[ntex::test]
  async fn basic() {
    const CARGO_NAME: &str = "client-test-cargo";
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    client.list_cargo(None).await.unwrap();
    let new_cargo = cargo_spec(CARGO_NAME);
    client.create_cargo(&new_cargo, None).await.unwrap();
    let cargo_key = format!("global.{CARGO_NAME}");
    client.start_process("cargo", &cargo_key).await.unwrap();
    client.inspect_cargo(&cargo_key).await.unwrap();
    let mut replacement =
      container_spec("main", "ghcr.io/next-hat/nanocl-get-started:latest");
    replacement.container_config.env = Some(vec!["TEST=1".into()]);
    let cargo_update = CargoSpecPatch {
      containers: Some(vec![replacement]),
      ..Default::default()
    };
    client.patch_cargo(&cargo_key, &cargo_update).await.unwrap();
    client.put_cargo(&cargo_key, &new_cargo).await.unwrap();
    let histories = client.list_history_cargo(&cargo_key).await.unwrap();
    assert!(histories.len() > 1);
    let history = histories
      .iter()
      .find(|history| history.containers != new_cargo.containers)
      .expect("Expected to find the patched Cargo declaration")
      .clone();
    let reverted = client
      .revert_cargo(&cargo_key, &history.key.to_string())
      .await
      .unwrap();
    assert_eq!(reverted.spec.name, history.name);
    assert_eq!(reverted.spec.metadata, history.metadata);
    assert_eq!(reverted.spec.replicas, history.replicas);
    assert_eq!(reverted.spec.network_mode, history.network_mode);
    assert_eq!(reverted.spec.port_bindings, history.port_bindings);
    assert_eq!(reverted.spec.secrets, history.secrets);
    assert_eq!(reverted.spec.placement, history.placement);
    assert_eq!(
      reverted.spec.resource_requirement,
      history.resource_requirement
    );
    assert_eq!(reverted.spec.init_containers, history.init_containers);
    assert_eq!(reverted.spec.containers, history.containers);
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
    let new_cargo = cargo_spec("client-test-cargodup");
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
