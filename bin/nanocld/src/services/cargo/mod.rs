use ntex::web;

pub mod count;
pub mod create;
pub mod delete;
pub mod inspect;
pub mod list;
pub mod list_history;
pub mod patch;
pub mod put;
pub mod revert;

pub use count::*;
pub use create::*;
pub use delete::*;
pub use inspect::*;
pub use list::*;
pub use list_history::*;
pub use patch::*;
pub use put::*;
pub use revert::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_cargo);
  config.service(delete_cargo);
  config.service(patch_cargo);
  config.service(put_cargo);
  config.service(list_cargo);
  config.service(inspect_cargo);
  config.service(list_cargo_history);
  config.service(revert_cargo);
  config.service(count_cargo);
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use nanocl_stubs::cargo::{
    Cargo, CargoDeleteQuery, CargoInspect, CargoKillOptions, CargoSummary,
  };
  use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPartial};

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/cargoes";

  /// Test to create start patch stop and delete a cargo with valid data
  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let test_cargoes = [
      "1daemon-test-cargo",
      "2another-test-cargo",
      "2daemon-test-cargo",
    ];
    let main_test_cargo = test_cargoes[0];
    for test_cargo in test_cargoes.iter() {
      let test_cargo = test_cargo.to_owned();
      let res = client
        .send_post(
          ENDPOINT,
          Some(&CargoSpecPartial {
            name: test_cargo.to_owned(),
            container: bollard_next::container::Config {
              image: Some(
                "ghcr.io/next-hat/nanocl-get-started:latest".to_owned(),
              ),
              ..Default::default()
            },
            ..Default::default()
          }),
          None::<String>,
        )
        .await;
      test_status_code!(
        res.status(),
        http::StatusCode::CREATED,
        "basic cargo create"
      );
      let cargo = TestClient::res_json::<Cargo>(res).await;
      assert_eq!(cargo.spec.name, test_cargo, "Invalid cargo name");
      assert_eq!(cargo.namespace_name, "global", "Invalid cargo namespace");
      assert_eq!(
        cargo.spec.container.image,
        Some("ghcr.io/next-hat/nanocl-get-started:latest".to_owned())
      );
    }
    let mut res = client
      .send_get(
        &format!("{ENDPOINT}/{main_test_cargo}/inspect"),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo inspect"
    );
    let response = res.json::<CargoInspect>().await.unwrap();
    assert_eq!(
      response.spec.name, main_test_cargo,
      "Expected to find cargo with name {main_test_cargo} got {}",
      response.spec.name
    );
    let mut res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo list");
    let cargoes = res.json::<Vec<CargoSummary>>().await.unwrap();
    assert!(!cargoes.is_empty(), "Expected to find cargoes");
    let res = client
      .send_post(
        &format!("/processes/cargo/{main_test_cargo}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "basic cargo start"
    );
    let res = client
      .send_post(
        &format!("/processes/cargo/{main_test_cargo}/kill"),
        Some(&CargoKillOptions {
          signal: "SIGINT".to_owned(),
        }),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo kill");
    let res = client
      .send_post(
        &format!("/processes/cargo/{main_test_cargo}/restart"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "basic cargo restart"
    );
    let mut res = client
      .send_put(
        &format!("{ENDPOINT}/{main_test_cargo}"),
        Some(&CargoSpecPartial {
          name: main_test_cargo.to_owned(),
          container: bollard_next::container::Config {
            image: Some(
              "ghcr.io/next-hat/nanocl-get-started:latest".to_owned(),
            ),
            env: Some(vec!["TEST=1".to_owned()]),
            ..Default::default()
          },
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo patch");
    let patch_response = res.json::<Cargo>().await.unwrap();
    assert_eq!(patch_response.spec.name, main_test_cargo);
    assert_eq!(patch_response.namespace_name, "global");
    assert_eq!(
      patch_response.spec.container.image,
      Some("ghcr.io/next-hat/nanocl-get-started:latest".to_owned())
    );
    assert_eq!(
      patch_response.spec.container.env,
      Some(vec!["TEST=1".to_owned()])
    );
    let mut res = client
      .send_get(
        &format!("{ENDPOINT}/{main_test_cargo}/histories"),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "basic cargo history"
    );
    let histories = res.json::<Vec<CargoSpec>>().await.unwrap();
    assert!(histories.len() > 1, "Expected to find cargo histories");
    let id = histories[0].key;
    let res = client
      .send_patch(
        &format!("{ENDPOINT}/{main_test_cargo}/histories/{id}/revert"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "basic cargo revert");
    let res = client
      .send_post(
        &format!("/processes/cargo/{main_test_cargo}/stop"),
        None::<String>,
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "basic cargo stop"
    );
    for test_cargo in test_cargoes.iter() {
      let res = client
        .send_delete(
          &format!("{ENDPOINT}/{test_cargo}"),
          Some(CargoDeleteQuery {
            force: Some(true),
            ..Default::default()
          }),
        )
        .await;
      test_status_code!(
        res.status(),
        http::StatusCode::ACCEPTED,
        "basic cargo delete"
      );
    }
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }
}
