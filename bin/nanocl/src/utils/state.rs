use regex::Regex;

use nanocld_client::stubs::state::StateConfig;

use crate::error::CliError;

pub fn get_file_meta(data: &str) -> Result<StateConfig, CliError> {
  let meta = serde_yaml::from_str::<StateConfig>(data)?;

  Ok(meta)
}

pub fn compile(data: &str, obj: &liquid::Object) -> Result<String, CliError> {
  // replace "${{ }}" with "{{ }}" syntax for liquid
  let reg = Regex::new(r"\$\{\{(.+?)\}\}").map_err(|err| CliError::Custom {
    msg: format!("{err}"),
  })?;
  let template = reg.replace_all(data, "{{ $1 }}").to_string();

  let template = liquid::ParserBuilder::with_stdlib()
    .build()
    .unwrap()
    .parse(&template)
    .map_err(|err| CliError::Custom {
      msg: format!("{err}"),
    })?;

  let output = template.render(&obj).map_err(|err| CliError::Custom {
    msg: format!("{err}"),
  })?;
  Ok(output)
}
