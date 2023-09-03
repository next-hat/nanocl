use ring::digest;

/// ## Calculate SHA256
///
/// Calculate the SHA256 hash of a string
///
/// ## Arguments
///
/// - [name](str) The string to hash
///
/// ## Returns
///
/// - [String](String) The hash
///
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
