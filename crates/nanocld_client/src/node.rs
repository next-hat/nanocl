use nanocl_error::http_client::HttpClientResult;

use nanocl_stubs::node::{DeleteNodeCargoParams, Node, StartNodeCargoParams};

use super::http_client::NanocldClient;

impl NanocldClient {
  /// ## Default path for nodes
  const NODE_PATH: &'static str = "/nodes";

  /// List existing nodes in the system
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let res = client.list_node().await;
  /// ```
  ///
  pub async fn list_node(&self) -> HttpClientResult<Vec<Node>> {
    let res = self.send_get(Self::NODE_PATH, None::<String>).await?;
    Self::res_json(res).await
  }

  /// Start cargo instances on a node
  ///
  /// ## Arguments
  ///
  /// * `params` - Parameters for starting the cargo
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocl_stubs::node::{CargoReplicaTask, StartNodeCargoParams};
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let params = StartNodeCargoParams {
  ///   cargo_key: "global.my-cargo".to_string(),
  ///   replicas: vec![CargoReplicaTask {
  ///     key: "3e8f6eef-f03a-446d-9e7a-635728c3f565".parse().unwrap(),
  ///     ordinal: 0,
  ///   }],
  /// };
  /// let res = client.start_node_cargo(&params).await;
  /// ```
  ///
  pub async fn start_node_cargo(
    &self,
    params: &StartNodeCargoParams,
  ) -> HttpClientResult<()> {
    let path = format!("{}/cargoes/start", Self::NODE_PATH);
    self.send_post(&path, Some(params), None::<String>).await?;
    Ok(())
  }

  /// Delete cargo instances on a node
  ///
  /// ## Arguments
  ///
  /// * `params` - Parameters for deleting the cargo
  ///
  /// ## Example
  ///
  /// ```no_run,ignore
  /// use nanocld_client::NanocldClient;
  /// use nanocl_stubs::node::DeleteNodeCargoParams;
  ///
  /// let client = NanocldClient::connect_to("http://localhost:8585", None);
  /// let params = DeleteNodeCargoParams {
  ///   cargo_key: "global.my-cargo".to_string(),
  /// };
  /// let res = client.delete_node_cargo(&params).await;
  /// ```
  ///
  pub async fn delete_node_cargo(
    &self,
    params: &DeleteNodeCargoParams,
  ) -> HttpClientResult<()> {
    let path = format!("{}/cargoes", Self::NODE_PATH);
    self.send_delete(&path, Some(params)).await?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use ntex::web;
  use serde_json::Value;

  use nanocl_stubs::node::CargoReplicaTask;

  use crate::ConnectOpts;

  use super::*;

  async fn record_start_node_cargo(
    recorder: web::types::State<Arc<Mutex<Option<Value>>>>,
    body: web::types::Json<Value>,
  ) -> web::HttpResponse {
    *recorder.lock().unwrap() = Some(body.into_inner());
    web::HttpResponse::Ok().finish()
  }

  #[ntex::test]
  async fn start_node_cargo_posts_the_exact_replica_vector_as_json() {
    let recorded = Arc::new(Mutex::new(None::<Value>));
    let server_recorded = recorded.clone();
    let server = web::test::server(async move || {
      web::App::new().state(server_recorded.clone()).service(
        web::resource("/v0.18.0/nodes/cargoes/start")
          .route(web::post().to(record_start_node_cargo)),
      )
    })
    .await;
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: format!("http://localhost:{}", server.addr().port()),
      ..Default::default()
    })
    .unwrap();
    let params = StartNodeCargoParams {
      cargo_key: "global.api".to_owned(),
      replicas: vec![
        CargoReplicaTask {
          key: "3e8f6eef-f03a-446d-9e7a-635728c3f565".parse().unwrap(),
          ordinal: 0,
        },
        CargoReplicaTask {
          key: "25a8057e-7147-4d26-9de5-652cafbf119f".parse().unwrap(),
          ordinal: 4,
        },
      ],
    };

    client.start_node_cargo(&params).await.unwrap();

    assert_eq!(
      recorded.lock().unwrap().clone().unwrap(),
      serde_json::to_value(params).unwrap()
    );
  }

  #[ntex::test]
  async fn basic() {
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    let node = client.list_node().await;
    assert!(node.is_ok());
  }
}
