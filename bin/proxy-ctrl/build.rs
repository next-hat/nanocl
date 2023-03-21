include!("src/models.rs");

#[cfg(feature = "bschemars")]
fn build_models() {
  use std::fs;

  let schema = schemars::schema_for!(ResourceProxyRule);
  let s = serde_yaml::to_string(&schema).unwrap();
  fs::write("specs/proxy_rule.yml", s).unwrap();
}

#[cfg(not(feature = "bschemars"))]
fn build_models() {}

fn main() {
  build_models();
}
