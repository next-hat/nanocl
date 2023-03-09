use std::io::Write;

use schemars::schema_for;

include!("./src/resource.rs");

fn main() -> std::io::Result<()> {
  let schema = schema_for!(ResourceProxyRule);
  let s = serde_yaml::to_string(&schema).unwrap();

  let mut file = std::fs::File::create("../../spec/resource_proxy_rule.yaml")?;
  file.write_all(s.as_bytes())?;

  Ok(())
}
