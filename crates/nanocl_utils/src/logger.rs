use env_logger;

pub fn enable_logger(bin_name: &str) {
  if std::env::var("LOG_LEVEL").is_err() {
    std::env::set_var("LOG_LEVEL", format!("{bin_name}=debug"));
  }
  let is_test = std::env::var("TEST").is_ok();
  env_logger::Builder::new()
    .parse_env("LOG_LEVEL")
    // .format_target(false)
    .is_test(is_test)
    .init();
}
