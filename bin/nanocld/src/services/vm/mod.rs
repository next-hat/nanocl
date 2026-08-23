use ntex::web;

pub mod count;
pub mod create;
pub mod delete;
pub mod inspect;
pub mod list;
pub mod list_history;
pub mod patch;

pub use count::*;
pub use create::*;
pub use delete::*;
pub use inspect::*;
pub use list::*;
pub use list_history::*;
pub use patch::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_vm);
  config.service(create_vm);
  config.service(delete_vm);
  config.service(inspect_vm);
  config.service(count_vm);
  config.service(list_vm_history);
  config.service(patch_vm);
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::{
    generic::{
      GenericClause, GenericCount, GenericFilter, GenericListQueryNsp,
      GenericNspQuery,
    },
    namespace::NamespacePartial,
    vm::{VmInspect, VmSummary},
    vm_spec::{VmSpecPartial, VmSpecUpdate},
  };
  use ntex::http;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn collection_namespace_and_key_semantics() {
    const NAME: &str = "same-name-vm-test";
    const NAMESPACE: &str = "vm-collection-test";
    const GLOBAL_KEY: &str = "global.same-name-vm-test";
    const OTHER_KEY: &str = "vm-collection-test.same-name-vm-test";

    let system = gen_default_test_system().await;
    let client = system.client;
    let response = client
      .send_post(
        "/namespaces",
        Some(NamespacePartial {
          name: NAMESPACE.to_owned(),
          metadata: None,
        }),
        None::<String>,
      )
      .await;
    assert!(
      response.status().is_success()
        || response.status() == http::StatusCode::CONFLICT
    );

    let spec = VmSpecPartial {
      name: NAME.to_owned(),
      image: "/tmp/namespace-test-vm.img".to_owned(),
      ..Default::default()
    };
    for namespace in [None, Some(NAMESPACE)] {
      let response = client
        .send_post("/vms", Some(&spec), Some(GenericNspQuery::new(namespace)))
        .await;
      assert!(
        response.status().is_success()
          || response.status() == http::StatusCode::CONFLICT
      );
    }

    let filter =
      GenericFilter::new().r#where("name", GenericClause::Eq(NAME.to_owned()));
    let query = |namespace: Option<&str>| {
      let mut query = GenericListQueryNsp::try_from(filter.clone()).unwrap();
      query.namespace = namespace.map(str::to_owned);
      query
    };

    for namespace in [None, Some(""), Some(" \t ")] {
      let response = client.send_get("/vms", Some(query(namespace))).await;
      let vms = response.json::<Vec<VmSummary>>().await.unwrap();
      let keys = vms
        .iter()
        .map(|vm| vm.spec.vm_key.as_str())
        .collect::<Vec<_>>();
      assert!(keys.contains(&GLOBAL_KEY));
      assert!(keys.contains(&OTHER_KEY));

      let response =
        client.send_get("/vms/count", Some(query(namespace))).await;
      let count = response.json::<GenericCount>().await.unwrap();
      assert_eq!(count.count as usize, vms.len());
    }

    for (namespace, expected_key) in
      [("global", GLOBAL_KEY), (NAMESPACE, OTHER_KEY)]
    {
      let response =
        client.send_get("/vms", Some(query(Some(namespace)))).await;
      let vms = response.json::<Vec<VmSummary>>().await.unwrap();
      assert_eq!(vms.len(), 1);
      assert_eq!(vms[0].spec.vm_key, expected_key);

      let response = client
        .send_get("/vms/count", Some(query(Some(namespace))))
        .await;
      let count = response.json::<GenericCount>().await.unwrap();
      assert_eq!(count.count, 1);
    }

    let response = client
      .send_get(&format!("/vms/{OTHER_KEY}/inspect"), None::<String>)
      .await;
    let vm = response.json::<VmInspect>().await.unwrap();
    assert_eq!(vm.namespace_name, NAMESPACE);

    let response = client
      .send_get("/vms/not-a-key/inspect", None::<String>)
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::BAD_REQUEST,
      "invalid VM key"
    );

    let response = client
      .send_patch(
        &format!("/vms/{OTHER_KEY}"),
        Some(VmSpecUpdate {
          name: Some("renamed".to_owned()),
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::BAD_REQUEST,
      "VM name is immutable"
    );

    let response = client
      .send_patch(
        &format!("/vms/{OTHER_KEY}"),
        Some(VmSpecUpdate {
          hostname: Some("updated".to_owned()),
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    test_status_code!(
      response.status(),
      http::StatusCode::OK,
      "patch VM by canonical key"
    );

    for key in [GLOBAL_KEY, OTHER_KEY] {
      let response = client
        .send_delete(&format!("/vms/{key}"), None::<String>)
        .await;
      test_status_code!(
        response.status(),
        http::StatusCode::OK,
        "delete VM by canonical key"
      );
    }
  }

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let name = "api-test-vm";
    let key = "global.api-test-vm";
    let image = "/tmp/ubuntu-22-test.img";
    let res = client
      .post("/vms")
      .send_json(&VmSpecPartial {
        name: name.to_owned(),
        image: image.to_owned(),
        ..Default::default()
      })
      .await
      .unwrap();
    let status = res.status();
    if status != http::StatusCode::OK {
      let body = res.json::<serde_json::Value>().await.unwrap();
      panic!("create vm failed: {} {}", status, body);
    }
    test_status_code!(res.status(), http::StatusCode::OK, "create vm");
    let res = client
      .get(&format!("/vms/{key}/inspect"))
      .send()
      .await
      .unwrap();
    test_status_code!(res.status(), http::StatusCode::OK, "inspect vm");
    let vm = res.json::<VmInspect>().await.unwrap();
    assert_eq!(vm.spec.name, name);
    let res = client.get("/vms").send().await.unwrap();
    test_status_code!(res.status(), http::StatusCode::OK, "list vm");
    let vms = res.json::<Vec<VmSummary>>().await.unwrap();
    assert!(vms.iter().any(|i| i.spec.name == name));
    let res = client.delete(&format!("/vms/{key}")).send().await.unwrap();
    test_status_code!(res.status(), http::StatusCode::OK, "delete vm");
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }
}
