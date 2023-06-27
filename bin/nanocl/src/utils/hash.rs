use ring::digest;

#[allow(non_snake_case)]
pub fn calculate_SHA256(name: &str) -> String {
  let mut context = digest::Context::new(&digest::SHA256);
  context.update(name.as_bytes());
  let hash_value: digest::Digest = context.finish();
  hash_value
    .as_ref()
    .iter()
    .map(|byte| format!("{:02x}", byte))
    .collect::<String>()
}
