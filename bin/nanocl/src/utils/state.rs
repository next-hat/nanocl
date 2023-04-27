use regex::Regex;

use nanocld_client::stubs::state::StateMeta;

use nanocl_utils::io_error::{IoError, IoResult, FromIo};

/// Extract metadata eg: ApiVersion, Type from a StateFile
pub fn get_file_meta(data: &str) -> IoResult<StateMeta> {
  let meta = serde_yaml::from_str::<StateMeta>(data).map_err(|err| {
    err.map_err_context(|| "Unable to extract meta from state file")
  })?;

  Ok(meta)
}

/// Compile a StateFile with given data
pub fn compile(data: &str, obj: &liquid::Object) -> IoResult<String> {
  // replace "${{ }}" with "{{ }}" syntax for liquid
  let reg = Regex::new(r"\$\{\{(.+?)\}\}")
    .map_err(|err| IoError::invalid_data("Regex", &format!("{err}")))?;
  let template = reg.replace_all(data, "{{ $1 }}").to_string();

  let template = liquid::ParserBuilder::with_stdlib()
    .build()
    .unwrap()
    .parse(&template)
    .map_err(|err| {
      IoError::invalid_data("Template parsing", &format!("{err}"))
    })?;

  let output = template.render(&obj).map_err(|err| {
    IoError::invalid_data("Template rendering", &format!("{err}"))
  })?;
  Ok(output)
}
