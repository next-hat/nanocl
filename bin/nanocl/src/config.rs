use std::fs;

pub struct CliConfig {
  pub url: Option<String>,
  pub ssl_cert: Option<String>,
  pub ssl_key: Option<String>,
  pub ssl_ca: Option<String>,
}

// pub fn read_cli_conf() -> CliConfig {
//   // // Get user config path
//   // std::env::h

//   // fs::read_to_string(path)
// }
