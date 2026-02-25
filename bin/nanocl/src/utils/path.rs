use nanocl_error::io::{IoError, IoResult};

pub fn resolve_full_path(path: &str) -> IoResult<String> {
  if !std::path::Path::new(path).exists() {
    return Err(IoError::not_found(
      &format!("Path {path}"),
      &"Does not exist".to_string(),
    ));
  }
  let full_path = std::fs::canonicalize(path).map_err(|err| {
    IoError::invalid_input(&format!("Path {path}"), &err.to_string())
  })?;
  let full_path = full_path
    .to_str()
    .ok_or_else(|| {
      IoError::invalid_input(
        &format!("Path {path}"),
        &"Invalid UTF-8 sequence".to_string(),
      )
    })?
    .to_owned();
  Ok(full_path)
}
