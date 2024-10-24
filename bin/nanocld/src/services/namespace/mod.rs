use ntex::web;

pub mod count;
pub mod create;
pub mod delete;
pub mod inspect;
pub mod list;

pub use count::*;
pub use create::*;
pub use delete::*;
pub use inspect::*;
pub use list::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_namespace);
  config.service(create_namespace);
  config.service(inspect_namespace);
  config.service(delete_namespace);
  config.service(count_namespace);
}

#[cfg(test)]
mod test_namespace {
  use serde_json::json;

  use nanocl_stubs::namespace::{Namespace, NamespacePartial};

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/namespaces";

  async fn list(client: &TestClient) {
    let res = client.send_get(ENDPOINT, None::<String>).await;
    assert!(res.status().is_success(), "Expect success on list");
    let _ = TestClient::res_json::<Vec<Namespace>>(res).await;
  }

  async fn create(client: &TestClient) {
    let new_namespace = NamespacePartial {
      name: String::from("controller-default"),
      metadata: None,
    };
    let res = client
      .send_post(ENDPOINT, Some(new_namespace), None::<String>)
      .await;
    assert!(res.status().is_success(), "Expect success on create");
  }

  async fn test_fail_create(client: &TestClient) {
    let res = client
      .send_post(
        ENDPOINT,
        Some(&json!({
          "name": 1,
        })),
        None::<String>,
      )
      .await;
    assert!(
      res.status().is_client_error(),
      "Expect error for invalid body"
    );
    let res = client
      .send_post(ENDPOINT, None::<String>, None::<String>)
      .await;
    assert!(res.status().is_client_error(), "Expect error when no body");
  }

  async fn inspect_by_id(client: &TestClient) {
    const NAME: &str = "controller-default";
    let res = client
      .send_get(&format!("{ENDPOINT}/{NAME}/inspect"), None::<String>)
      .await;
    assert!(res.status().is_success(), "Expect success on inspect_by_id");
  }

  async fn delete(client: &TestClient) {
    const NAME: &str = "controller-default";
    let res = client
      .send_delete(&format!("{ENDPOINT}/{NAME}"), None::<String>)
      .await;
    assert!(res.status().is_success(), "Expect success on delete");
  }

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    test_fail_create(&client).await;
    create(&client).await;
    inspect_by_id(&client).await;
    list(&client).await;
    delete(&client).await;
    system.state.wait_event_loop().await;
  }
}
