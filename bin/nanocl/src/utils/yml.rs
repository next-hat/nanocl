use crate::error::CliError;
use crate::models::{YmlFile, YmlResource};

pub fn get_file_meta(data: &str) -> Result<YmlFile, CliError> {
  let meta = serde_yaml::from_str::<YmlFile>(data)?;

  Ok(meta)
}

pub fn get_resources(data: &str) -> Result<YmlResource, CliError> {
  let yml_resources = serde_yaml::from_str::<YmlResource>(data)?;

  Ok(yml_resources)
}
