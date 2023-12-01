/// Utils to manipulate `key` property of a model
/// The key property is based on the `namespace` and the `name` of the given model
/// or on the key of the parent for relational purpose.
/// For example if we create a cargo `get-started` in the default namespace `global`
/// The cargo key will be `get-started.global`
use ntex::http;

use nanocl_error::http::{HttpError, HttpResult};

/// Resolve the namespace from the query paramater
/// Namespace is an optional query paramater it's resolved with value `global` if it's empty
pub(crate) fn resolve_nsp(nsp: &Option<String>) -> String {
  match nsp {
    None => String::from("global"),
    Some(nsp) => nsp.to_owned(),
  }
}

/// Generate a key based on the namespace and the name of the model.
pub(crate) fn gen_key(nsp: &str, name: &str) -> String {
  name.to_owned() + "." + nsp
}

/// Validate the name of a cargo or a vm
/// By checking if it's only contain a-z, A-Z, 0-9, - and _
pub(crate) fn validate_name(name: &str) -> HttpResult<()> {
  // Ensure name only contain a-z, A-Z, 0-9, - and _
  if !name
    .chars()
    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
  {
    return Err(HttpError {
      status: http::StatusCode::BAD_REQUEST,
      msg: format!("Vm image name {name} is invalid"),
    });
  }
  Ok(())
}
