pub use ntex::web;

pub mod count;
pub mod list;
pub mod ws;

pub use count::*;
pub use list::*;
pub use ws::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_node);
  config.service(count_node);
  config.service(web::resource("/nodes/ws").route(web::get().to(node_ws)));
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
