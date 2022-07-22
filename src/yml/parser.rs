use super::models;

pub fn get_config_type(
  str: &str,
) -> Result<models::YmlConfigTypes, serde_yaml::Error> {
  let result = serde_yaml::from_str::<models::YmlFile>(str)?;
  Ok(result.file_type)
}
