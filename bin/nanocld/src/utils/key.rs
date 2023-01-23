/// Utils to manipulate `key` property of a model
/// The key property is based on the namespace and the name of the given model
/// or on the key of the parent for relational purpose.
/// For example if we create a cargo `get-started` in the default namespace `global`
/// The cargo key will be `global-get-started`

/// ## Resolve nsp
///
/// Resolve the namespace from the query paramater
/// Namespace is an optional query paramater it's resolved with value `global` if it's empty
///
/// ## Arguments
///
/// - [nsp](Option<String>) The namespace to resolve
///
/// ## Return
///
/// - [namespace](String) The resolved namespace
///
/// ## Example
/// ```rust,norun
/// let nsp = resolve_nsp(None); // return "global"
/// ```
///
pub fn resolve_nsp(nsp: &Option<String>) -> String {
  match nsp {
    None => String::from("global"),
    Some(nsp) => nsp.to_owned(),
  }
}

/// ## Generate key
/// Return the generated key from the namespace and the name of an item
///
/// ## Arguments
///
/// - [m1](str)  The key of the first model
/// - [m2](str) The name of the second model
///
/// ## Return
///
/// - [key](String) The generated key based on params
///
/// ## Example
///
/// ```rust,norun
/// let key = gen_key("global", "front");
/// ```
///
pub fn gen_key(nsp: &str, name: &str) -> String {
  nsp.to_owned() + "-" + name
}
