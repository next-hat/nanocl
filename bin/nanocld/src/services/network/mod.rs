use ntex::web;

mod inspect;
mod list;

pub use inspect::*;
pub use list::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_network);
  config.service(inspect_network);
}

#[cfg(test)]
mod tests {
  use bollard_next::service::Network as DockerNetwork;
  use nanocl_stubs::{
    generic::{GenericClause, GenericFilter, GenericListQuery},
    network::{Network, NetworkPartial},
  };
  use ntex::http;

  use crate::{
    models::{NetworkDb, NodeDb},
    repositories::generic::*,
    utils::tests::*,
  };

  #[ntex::test]
  async fn list_inspect_and_preserve_metadata_on_refresh() {
    let system = gen_default_test_system().await;
    let node_name = format!("network-test-{}", uuid::Uuid::new_v4());
    NodeDb::create_if_not_exists(
      &NodeDb {
        name: node_name.clone(),
        created_at: chrono::Utc::now().naive_utc(),
        endpoint: "unix:///var/run/docker.sock".to_owned(),
        version: crate::vars::VERSION.to_owned(),
        metadata: None,
      },
      &system.state.inner.pool,
    )
    .await
    .unwrap();

    let mut initial = NetworkPartial::new(
      &node_name,
      "private-api",
      DockerNetwork {
        name: Some("private-api".to_owned()),
        driver: Some("bridge".to_owned()),
        ..Default::default()
      },
    );
    initial.metadata = Some(serde_json::json!({ "owner": "test" }));
    NetworkDb::upsert(&initial, &system.state.inner.pool)
      .await
      .unwrap();

    let mut refreshed = initial.clone();
    refreshed.data.driver = Some("overlay".to_owned());
    refreshed.metadata = Some(serde_json::json!({ "owner": "overwritten" }));
    NetworkDb::upsert(&refreshed, &system.state.inner.pool)
      .await
      .unwrap();

    let filter = GenericFilter::new()
      .r#where("node_name", GenericClause::Eq(node_name.clone()));
    let query = GenericListQuery::try_from(filter).unwrap();
    let res = system.client.send_get("/networks", Some(query)).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list networks");
    let networks = res.json::<Vec<Network>>().await.unwrap();
    assert_eq!(networks.len(), 1);
    assert_eq!(networks[0].data.driver.as_deref(), Some("overlay"));
    assert_eq!(
      networks[0].metadata,
      Some(serde_json::json!({ "owner": "test" }))
    );

    let res = system
      .client
      .send_get(
        &format!("/networks/{}/inspect", initial.key),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "inspect network");
    let network = res.json::<Network>().await.unwrap();
    assert_eq!(network.key, initial.key);
    assert_eq!(network.node_name, node_name);

    NodeDb::del_by_pk(&node_name, &system.state.inner.pool)
      .await
      .unwrap();
  }
}
