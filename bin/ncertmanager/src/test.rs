#[cfg(test)]
pub(crate) mod tests {
  use nanocl_utils::logger;
  use nanocld_client::NanocldClient;

  // Before a test
  pub fn before() {
    // Build a test env logger
    std::env::set_var("TEST", "true");
    logger::enable_logger("ncertmanager");
  }

  pub async fn gen_default_test_client() -> NanocldClient {
    before();

    NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None)
  }
}
