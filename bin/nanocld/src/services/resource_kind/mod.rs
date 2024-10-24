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
  config
    .service(list_resource_kind)
    .service(create_resource_kind)
    .service(delete_resource_kind)
    .service(inspect_resource_kind)
    .service(count_resource_kind)
    .service(inspect_resource_kind_version);
}

#[cfg(test)]
mod tests {
  use ntex::http;

  const ENDPOINT: &str = "/resource/kinds";

  use crate::utils::tests::*;

  use nanocl_stubs::resource_kind::{
    ResourceKind, ResourceKindInspect, ResourceKindPartial, ResourceKindSpec,
    ResourceKindVersion,
  };

  #[ntex::test]
  async fn test_inspect_version_not_found() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client
      .send_get(
        &format!("{}/test.io/api-test/version/v12/inspect", ENDPOINT),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "resource kind inspect version"
    );
  }

  #[ntex::test]
  async fn test_wrong_name() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let payload = ResourceKindPartial {
      name: "api-test".to_owned(),
      version: "v1".to_owned(),
      metadata: None,
      data: ResourceKindSpec {
        schema: None,
        url: Some("unix:///run/nanocl/proxy.sock".to_owned()),
      },
    };
    let res = client
      .send_post(ENDPOINT, Some(&payload), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "resource kind create"
    );
  }

  #[ntex::test]
  async fn test_wrong_spec() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let payload = ResourceKindPartial {
      name: "test.io/api-test".to_owned(),
      version: "v1".to_owned(),
      metadata: None,
      data: ResourceKindSpec {
        schema: None,
        url: None,
      },
    };
    let res = client
      .send_post(ENDPOINT, Some(&payload), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "resource kind create"
    );
  }
  #[ntex::test]
  async fn basic_list() {
    let system = gen_default_test_system().await;
    let client = system.client;
    // Create
    let payload = ResourceKindPartial {
      name: "test.io/api-test".to_owned(),
      version: "v1".to_owned(),
      metadata: None,
      data: ResourceKindSpec {
        schema: None,
        url: Some("unix:///run/nanocl/proxy.sock".to_owned()),
      },
    };
    let mut res = client
      .send_post(ENDPOINT, Some(&payload), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::CREATED,
      "resource kind create"
    );
    let kind = res.json::<ResourceKind>().await.unwrap();
    assert_eq!(kind.name, payload.name);
    assert_eq!(kind.version, payload.version);
    // List
    let mut res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "resource kind list");
    let items = res.json::<Vec<ResourceKind>>().await.unwrap();
    assert!(items.iter().any(|i| i.name == payload.name));
    // Inspect
    let mut res = client
      .send_get(
        &format!("{}/{}/inspect", ENDPOINT, payload.name),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "resource kind inspect"
    );
    let kind = res.json::<ResourceKindInspect>().await.unwrap();
    assert_eq!(kind.name, payload.name);
    // Inspect version
    let mut res = client
      .send_get(
        &format!(
          "{}/{}/version/{}/inspect",
          ENDPOINT, payload.name, payload.version
        ),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "resource kind inspect version"
    );
    let _ = res.json::<ResourceKindVersion>().await.unwrap();
    // Delete
    let res = client
      .send_delete(&format!("{}/{}", ENDPOINT, payload.name), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "resource kind delete"
    );
  }
}
