pub use ntex::web;

pub mod count;
pub mod create;
pub mod delete;
pub mod inspect;
pub mod list;
pub mod patch;

pub use count::*;
pub use create::*;
pub use delete::*;
pub use inspect::*;
pub use list::*;
pub use patch::*;

pub fn ntex_config(
  config: &mut web::ServiceConfig<crate::models::SystemState>,
) {
  config.service(
    web::resource("/secrets")
      .name("list_secret")
      .guard(web::guard::Get())
      .to_with_state(list_secret),
  );
  config.service(
    web::resource("/secrets")
      .name("create_secret")
      .guard(web::guard::Post())
      .to_with_state(create_secret),
  );
  config.service(
    web::resource("/secrets/{key}/inspect")
      .name("inspect_secret")
      .guard(web::guard::Get())
      .to_with_state(inspect_secret),
  );
  config.service(
    web::resource("/secrets/{key}")
      .name("delete_secret")
      .guard(web::guard::Delete())
      .to_with_state(delete_secret),
  );
  config.service(
    web::resource("/secrets/count")
      .name("count_secret")
      .guard(web::guard::Get())
      .to_with_state(count_secret),
  );
  config.service(
    web::resource("/secrets/{key}")
      .name("patch_secret")
      .guard(web::guard::Patch())
      .to_with_state(patch_secret),
  );
}

#[cfg(test)]
mod test_secret {
  use ntex::http;

  use serde_json::json;

  use nanocl_stubs::secret::{Secret, SecretPartial};

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/secrets";

  async fn test_list(client: &TestClient) {
    let res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list secrets");
  }

  async fn test_create(client: &TestClient) {
    let new_secret = SecretPartial {
      name: String::from("test-secret"),
      kind: String::from("test-create.io/test"),
      immutable: false,
      data: json!({
        "Tls": { "cert": "MY CERT", "key": "MY KEY" },
      }),
      metadata: None,
    };
    let res = client
      .send_post(ENDPOINT, Some(new_secret), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::CREATED, "create secret");
    let _ = res.json::<Secret>().await.unwrap();
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
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "create secret with invalid body"
    );
    let res = client
      .send_post(ENDPOINT, None::<String>, None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "create secret with no body"
    );
  }

  async fn test_inspect_by_id(client: &TestClient) {
    let res = client
      .send_get(&format!("{ENDPOINT}/test-secret/inspect"), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "inspect secret");
  }

  async fn test_delete(client: &TestClient) {
    let res = client
      .send_delete(&format!("{ENDPOINT}/test-secret"), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "delete secret"
    );
  }

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    test_fail_create(&client).await;
    test_create(&client).await;
    test_inspect_by_id(&client).await;
    test_list(&client).await;
    test_delete(&client).await;
  }
}
