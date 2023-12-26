pub mod rule;
pub mod server;

#[cfg(test)]
pub mod tests {
  use nanocld_client::NanocldClient;

  pub use nanocl_utils::ntex::test_client::*;

  use crate::{version, services, models::Dnsmasq};

  // Before a test
  pub fn before() {
    // Build a test env logger
    std::env::set_var("TEST", "true");
  }

  // Generate a test server
  pub fn gen_default_test_client() -> TestClient {
    before();
    let dnsmasq = Dnsmasq::new("/tmp/dnsmasq");
    dnsmasq.ensure().unwrap();
    let client = NanocldClient::connect_to("http://nanocl.internal:8585", None);
    // Create test server
    let srv = ntex::web::test::server(move || {
      ntex::web::App::new()
        .state(dnsmasq.clone())
        .state(client.clone())
        .configure(services::ntex_config)
    });
    TestClient::new(srv, version::VERSION)
  }
}
