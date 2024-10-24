use ntex::web;

pub mod attach;
pub mod count;
pub mod create;
pub mod delete;
pub mod inspect;
pub mod list;
pub mod list_history;
pub mod patch;

pub use attach::*;
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
  config.service(
    web::resource("/vms/{name}/attach").route(web::get().to(vm_attach)),
  );
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::vm::{VmInspect, VmSummary};
  use nanocl_stubs::vm_spec::{VmDisk, VmSpecPartial};
  use ntex::http;

  use crate::services::vm_image::tests::ensure_test_image;
  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() {
    ensure_test_image().await;
    let system = gen_default_test_system().await;
    let client = system.client;
    let name = "api-test-vm";
    let image = "ubuntu-22-test";
    let mut res = client
      .post("/vms")
      .send_json(&VmSpecPartial {
        name: name.to_owned(),
        disk: VmDisk {
          image: image.to_owned(),
          ..Default::default()
        },
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
    let mut res = client
      .get(&format!("/vms/{name}/inspect"))
      .send()
      .await
      .unwrap();
    test_status_code!(res.status(), http::StatusCode::OK, "inspect vm");
    let vm = res.json::<VmInspect>().await.unwrap();
    assert_eq!(vm.spec.name, name);
    let mut res = client.get("/vms").send().await.unwrap();
    test_status_code!(res.status(), http::StatusCode::OK, "list vm");
    let vms = res.json::<Vec<VmSummary>>().await.unwrap();
    assert!(vms.iter().any(|i| i.spec.name == name));
    let res = client.delete(&format!("/vms/{name}")).send().await.unwrap();
    test_status_code!(res.status(), http::StatusCode::OK, "delete vm");
    ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    system.state.wait_event_loop().await;
  }
}
