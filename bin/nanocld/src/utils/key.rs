/// Utils to manipulate `key` property of a model
/// The key property is based on the `namespace` and the `name` of the given model
/// or on the key of the parent for relational purpose.
/// For example if we create a cargo `get-started` in the default namespace `global`
/// The cargo key will be `get-started.global`
use ntex::http;

use nanocl_error::http::HttpError;

/// ## Resolve nsp
///
/// Resolve the namespace from the query paramater
/// Namespace is an optional query paramater it's resolved with value `global` if it's empty
///
/// ## Arguments
///
/// * [nsp](Option<String>) The namespace to resolve
///
/// ## Returns
///
/// * [namespace](String) The resolved namespace
///
pub fn resolve_nsp(nsp: &Option<String>) -> String {
  match nsp {
    None => String::from("global"),
    Some(nsp) => nsp.to_owned(),
  }
}

/// ## Gen key
///
/// Generate a key based on the namespace and the name of the model.
///
/// ## Arguments
///
/// * [m1](str)  The key of the first model
/// * [m2](str) The name of the second model
///
/// ## Returns
///
/// * [key](String) The generated key based on params
///
pub fn gen_key(nsp: &str, name: &str) -> String {
  name.to_owned() + "." + nsp
}

/// ## Validate name
///
/// Validate the name of a cargo or a vm
/// By checking if it's only contain a-z, A-Z, 0-9, - and _
///
/// ## Arguments
///
/// * [name](str) The name to validate
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The name is valid
///   * [Err](HttpError) - The name is invalid
///
pub fn validate_name(name: &str) -> Result<(), HttpError> {
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
