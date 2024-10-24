use ntex::web;

pub mod count;
pub mod create;
pub mod delete;
pub mod inspect;
pub mod list;
pub mod list_history;
pub mod put;
pub mod revert;

pub use count::*;
pub use create::*;
pub use delete::*;
pub use inspect::*;
pub use list::*;
pub use list_history::*;
pub use put::*;
pub use revert::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_resource);
  config.service(delete_resource);
  config.service(list_resource);
  config.service(inspect_resource);
  config.service(put_resource);
  config.service(count_resource);
  config.service(list_resource_history);
  config.service(revert_resource);
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::{
    generic::{GenericClause, GenericFilter, GenericListQuery},
    resource::{Resource, ResourcePartial, ResourceUpdate},
    resource_kind::{ResourceKindPartial, ResourceKindSpec},
  };
  use ntex::http;

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/resources";

  #[ntex::test]
  async fn basic() {
    const TEST_RESOURCE: &str = "test_resource";
    const TEST_RESOURCE_KIND: &str = "test.io/test-resource";
    const TEST_RESOURCE_KIND_VERSION: &str = "v1";
    let system = gen_default_test_system().await;
    let client = system.client;
    let spec = serde_json::json!({
      "Schema": {
        "title": "VpnUser",
        "description": "Create a new vpn user",
        "type": "object",
        "required": [
          "Username"
        ],
        "properties": {
          "Username": {
            "description": "Username for the vpn user",
            "type": "string"
          },
          "Password": {
            "description": "Password for the vpn user",
            "type": "string"
          }
        }
      }
    });
    let payload = ResourceKindPartial {
      name: TEST_RESOURCE_KIND.to_owned(),
      version: TEST_RESOURCE_KIND_VERSION.to_owned(),
      metadata: None,
      data: ResourceKindSpec {
        schema: Some(spec),
        url: None,
      },
    };
    let res = client
      .send_post("/resource/kinds", Some(&payload), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::CREATED,
      "create resource kind"
    );
    let data = serde_json::json!({
      "Username": "test",
    });
    let resource = ResourcePartial {
      name: TEST_RESOURCE.to_owned(),
      kind: TEST_RESOURCE_KIND.to_owned(),
      data: data.clone(),
      metadata: Some(serde_json::json!({
        "Test": "gg",
      })),
    };
    let mut res = client
      .send_post(ENDPOINT, Some(&resource), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::CREATED,
      "create resource"
    );
    let resource = res.json::<Resource>().await.unwrap();
    assert_eq!(resource.spec.resource_key, TEST_RESOURCE);
    assert_eq!(resource.kind, TEST_RESOURCE_KIND);
    // Basic list
    let mut res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list resource");
    let _ = res.json::<Vec<Resource>>().await.unwrap();
    // Using filter exists
    let filter = GenericFilter::new()
      .r#where("data", GenericClause::HasKey("Username".to_owned()));
    let query = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get(ENDPOINT, Some(&query)).await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "filter resource by data HasKey"
    );
    let resources = res.json::<Vec<Resource>>().await.unwrap();
    assert!(
      resources.len() == 1,
      "Expect 1 resource when filter by data HasKey"
    );
    let filter = GenericFilter::new().r#where(
      "data",
      GenericClause::Contains(serde_json::json!({
        "Username": "test"
      })),
    );
    let query = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get(ENDPOINT, Some(&query)).await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "filter resource by data contains"
    );
    let resources = res.json::<Vec<Resource>>().await.unwrap();
    assert!(
      resources.len() == 1,
      "Expect 1 resource when filter by data contains"
    );
    let filter = GenericFilter::new()
      .r#where("metadata", GenericClause::HasKey("Test".to_owned()));
    let query = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get(ENDPOINT, Some(&query)).await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "filter resource by meta exists"
    );
    let resources = res.json::<Vec<Resource>>().await.unwrap();
    assert!(
      resources.len() == 1,
      "Expect 1 resource when filter by metadata HasKey"
    );
    let filter = GenericFilter::new().r#where(
      "metadata",
      GenericClause::Contains(serde_json::json!({
        "Test": "gg",
      })),
    );
    let query = GenericListQuery::try_from(filter).unwrap();
    let mut res = client.send_get(ENDPOINT, Some(&query)).await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "filter resource by meta contains"
    );
    let resources = res.json::<Vec<Resource>>().await.unwrap();
    assert!(
      resources.len() == 1,
      "Expect 1 resource when filter by meta contains"
    );
    // Inspect
    let mut res = client
      .send_get(
        &format!("{ENDPOINT}/{TEST_RESOURCE}/inspect"),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "inspect resource");
    let resource = res.json::<Resource>().await.unwrap();
    assert_eq!(resource.spec.resource_key, TEST_RESOURCE);
    assert_eq!(&resource.kind, TEST_RESOURCE_KIND);
    assert_eq!(&resource.spec.data, &data);
    // History
    let _ = client
      .send_get(
        &format!("{ENDPOINT}/{TEST_RESOURCE}/histories"),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "list resource history"
    );
    let data = serde_json::json!({
      "Username": "test_update",
    });
    let new_resource = ResourceUpdate {
      data: data.clone(),
      metadata: None,
    };
    let mut res = client
      .send_put(
        &format!("{ENDPOINT}/{TEST_RESOURCE}"),
        Some(&new_resource),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "patch resource");
    let resource = res.json::<Resource>().await.unwrap();
    assert_eq!(resource.spec.resource_key, TEST_RESOURCE);
    assert_eq!(&resource.kind, TEST_RESOURCE_KIND);
    // Delete
    let resp = client
      .send_delete(&format!("{ENDPOINT}/{TEST_RESOURCE}"), None::<String>)
      .await;
    test_status_code!(
      resp.status(),
      http::StatusCode::ACCEPTED,
      "delete resource"
    );
    let res = client
      .send_delete(
        &format!("/resource/kinds/{TEST_RESOURCE_KIND}"),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "delete resource kind"
    );
  }
}
