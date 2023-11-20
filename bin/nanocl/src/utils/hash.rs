use ring::digest;

/// ## Calculate SHA256
///
/// Calculate the SHA256 hash of a string
///
/// ## Arguments
///
/// * [name](str) The string to hash
///
/// ## Return
///
/// [String](String) The hash
///
#[allow(non_snake_case)]
pub fn calculate_SHA256(name: &str) -> String {
  let mut context = digest::Context::new(&digest::SHA256);
  context.update(name.as_bytes());
  let hash_value: digest::Digest = context.finish();
  hash_value
    .as_ref()
    .iter()
    .fold(String::new(), |acc, byte| format!("{acc}{:02x}", byte))
}

#[cfg(test)]
mod tests {
  #[test]
  fn sha256() {
    let hash = super::calculate_SHA256("test");
    assert_eq!(
      hash,
      "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    );
  }
}
