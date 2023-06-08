use liquid::ObjectView;
use regex::Regex;

use nanocld_client::stubs::state::StateMeta;

use nanocl_utils::io_error::{IoError, IoResult, FromIo};

/// Extract metadata eg: ApiVersion, Type from a Statefile
pub fn get_file_meta(ext: &str, data: &str) -> IoResult<StateMeta> {
  match ext {
    "yaml" | "yml" => {
      Ok(serde_yaml::from_str::<StateMeta>(data).map_err(|err| {
        err.map_err_context(|| "Unable to extract meta from state file")
      })?)
    }
    "json" => Ok(serde_json::from_str::<StateMeta>(data).map_err(|err| {
      err.map_err_context(|| "Unable to extract meta from state file")
    })?),
    "toml" => Ok(toml::from_str::<StateMeta>(data).map_err(|err| {
      IoError::invalid_data(
        "Unable to extract meta from state file",
        &err.to_string(),
      )
    })?),
    _ => Err(IoError::invalid_data(
      "State file",
      &format!("Unsupported file extension: {}", ext),
    )),
  }
}

/// Compile a Statefile with given data
pub fn compile(data: &str, obj: &dyn ObjectView) -> IoResult<String> {
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
