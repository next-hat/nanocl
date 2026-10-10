pub use ntex::web;

pub mod cargo;
pub mod count;
pub mod list;

pub use cargo::*;
pub use count::*;
pub use list::*;

pub fn ntex_config(
  config: &mut web::ServiceConfig<crate::models::SystemState>,
) {
  config
    .service(
      web::resource("/nodes/cargoes/start")
        .name("start_node_cargo")
        .guard(web::guard::Post())
        .to_with_state(start_node_cargo),
    )
    .service(
      web::resource("/nodes")
        .name("list_node")
        .guard(web::guard::Get())
        .to_with_state(list_node),
    )
    .service(
      web::resource("/nodes/count")
        .name("count_node")
        .guard(web::guard::Get())
        .to_with_state(count_node),
    );
}

#[cfg(test)]
mod tests {

  use ntex::http;

  use nanocl_stubs::node::Node;

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/nodes";

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list nodes");
    let _ = res.json::<Vec<Node>>().await.unwrap();
  }
}
