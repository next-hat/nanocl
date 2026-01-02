pub use ntex::web;

pub mod cargo;
pub mod count;
pub mod list;

pub use cargo::*;
pub use count::*;
pub use list::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config
    .service(start_node_cargo)
    .service(list_node)
    .service(count_node);
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
    let mut res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "list nodes");
    let _ = res.json::<Vec<Node>>().await.unwrap();
  }
}
